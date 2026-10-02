package ui

import (
	"context"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"tooln/internal/pkgmgr"
)

// Messages flowing back into the update loop. Every one carries the manager id
// it belongs to, so a reply that arrives after the user switched tabs is applied
// to the right place instead of the visible one.

type probeDoneMsg struct {
	mgr string
	err error
}

type listMsg struct {
	mgr  string
	pkgs []pkgmgr.Package
	err  error
}

type searchMsg struct {
	mgr   string
	query string
	pkgs  []pkgmgr.Package
	err   error
}

type detailsMsg struct {
	mgr     string
	name    string
	details *pkgmgr.Details
	err     error
}

type outdatedMsg struct {
	mgr      string
	outdated map[string]string
	err      error
}

// opDoneMsg reports the outcome of a mutating plan.
type opDoneMsg struct {
	mgr     string
	op      operation
	plan    pkgmgr.Plan
	targets []string
	err     error
	elapsed time.Duration

	// snapshot is the pip dependency graph as it looked before an uninstall, so
	// the follow-up pass can work out what became unused.
	snapshot pkgmgr.Snapshot
}

// orphansMsg carries dependencies that are no longer needed after a removal.
type orphansMsg struct {
	mgr     string
	removed []string
	orphans []string
	err     error
}

type logMsg struct{ line string }

type tickMsg time.Time

// operation identifies what a plan was doing, for status messages and for
// deciding what to do once it finishes.
type operation int

const (
	opInstall operation = iota
	opRemove
	opUpgrade
	opExtra
)

func (o operation) verb() string {
	switch o {
	case opInstall:
		return "install"
	case opRemove:
		return "uninstall"
	case opUpgrade:
		return "upgrade"
	default:
		return "run"
	}
}

func (o operation) past() string {
	switch o {
	case opInstall:
		return "installed"
	case opRemove:
		return "uninstalled"
	case opUpgrade:
		return "upgraded"
	default:
		return "completed"
	}
}

// ---------------------------------------------------------------- commands ----

func probeCmd(m pkgmgr.Manager) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
		defer cancel()
		return probeDoneMsg{mgr: m.ID(), err: m.Probe(ctx)}
	}
}

func listCmd(m pkgmgr.Manager) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Minute)
		defer cancel()
		pkgs, err := m.List(ctx)
		return listMsg{mgr: m.ID(), pkgs: pkgs, err: err}
	}
}

func searchCmd(m pkgmgr.Manager, query string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
		defer cancel()
		pkgs, err := m.Search(ctx, query)
		return searchMsg{mgr: m.ID(), query: query, pkgs: pkgs, err: err}
	}
}

func detailsCmd(m pkgmgr.Manager, p pkgmgr.Package) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
		defer cancel()
		d, err := m.Details(ctx, p)
		return detailsMsg{mgr: m.ID(), name: p.Name, details: d, err: err}
	}
}

func outdatedCmd(m pkgmgr.Manager) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Minute)
		defer cancel()
		out, err := m.Outdated(ctx)
		return outdatedMsg{mgr: m.ID(), outdated: out, err: err}
	}
}

// runPlanCmd executes a plan. For pip uninstalls it first records the
// dependency graph, so the orphan pass afterwards has something to compare
// against.
func runPlanCmd(m pkgmgr.Manager, log pkgmgr.Logger, op operation, plan pkgmgr.Plan, targets []string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Minute)
		defer cancel()

		start := time.Now()
		msg := opDoneMsg{mgr: m.ID(), op: op, plan: plan, targets: targets}

		if op == opRemove {
			if of, ok := m.(pkgmgr.OrphanFinder); ok {
				if snap, err := of.Snapshot(ctx); err == nil {
					msg.snapshot = snap
				} else {
					log("! could not record the dependency graph: " + err.Error())
				}
			}
		}
		msg.err = pkgmgr.RunPlan(ctx, log, plan)
		msg.elapsed = time.Since(start).Round(time.Millisecond)
		return msg
	}
}

// orphansCmd finds and removes dependencies left behind by an uninstall, so the
// requirement that "no-longer-needed dependencies no longer appear" holds.
func orphansCmd(m pkgmgr.Manager, log pkgmgr.Logger, before pkgmgr.Snapshot, removed []string) tea.Cmd {
	of, ok := m.(pkgmgr.OrphanFinder)
	if !ok || before == nil {
		return nil
	}
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
		defer cancel()

		orphans, err := of.Orphans(ctx, before, removed)
		if err != nil {
			return orphansMsg{mgr: m.ID(), removed: removed, err: err}
		}
		if len(orphans) == 0 {
			return orphansMsg{mgr: m.ID(), removed: removed}
		}
		log("# these dependencies are no longer required by anything installed:")
		for _, o := range orphans {
			log("#   " + o)
		}
		plan, err := of.RemovePlanMany(ctx, orphans)
		if err != nil {
			return orphansMsg{mgr: m.ID(), removed: removed, orphans: orphans, err: err}
		}
		if err := pkgmgr.RunPlan(ctx, log, plan); err != nil {
			return orphansMsg{mgr: m.ID(), removed: removed, orphans: orphans, err: err}
		}
		return orphansMsg{mgr: m.ID(), removed: removed, orphans: orphans}
	}
}

// predictOrphansCmd is the read-only preview used by the uninstall dialog.
func predictOrphansCmd(m pkgmgr.Manager, targets []string) tea.Cmd {
	pip, ok := m.(*pkgmgr.Pip)
	if !ok {
		return nil
	}
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
		defer cancel()
		snap, err := pip.Snapshot(ctx)
		if err != nil {
			return predictMsg{mgr: m.ID(), targets: targets, err: err}
		}
		orphans, err := pip.PredictOrphans(ctx, snap, targets)
		return predictMsg{mgr: m.ID(), targets: targets, orphans: orphans, err: err}
	}
}

// predictMsg carries the uninstall preview back to the dialog.
type predictMsg struct {
	mgr     string
	targets []string
	orphans []string
	err     error
}

func tickCmd() tea.Cmd {
	return tea.Tick(120*time.Millisecond, func(t time.Time) tea.Msg { return tickMsg(t) })
}
