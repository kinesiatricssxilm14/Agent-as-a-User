// Package pkgmgr wraps the real package managers available on the host (pip and
// apt) behind a single interface so the TUI can talk to both in the same way.
//
// Nothing in this package fakes state: every listing, detail view and mutation
// shells out to the underlying tool (python3 -m pip, dpkg-query, apt-get, ...)
// or queries PyPI over HTTP, and reports exactly what those tools said.
package pkgmgr

import (
	"context"
	"fmt"
	"sort"
	"strings"
)

// Package is one row in a package list.
type Package struct {
	Name    string // name as the manager knows it (used for install/remove)
	Version string // installed version, or candidate version for search hits
	Summary string

	Installed bool   // present in the live environment
	Latest    string // newer version available, when known (see Manager.Outdated)
	Note      string // short annotation shown in the list ("dependency", ...)
}

// Field is a single labelled value in a details pane.
type Field struct {
	Key   string
	Value string
}

// Details is the fully expanded description of one package.
type Details struct {
	Name       string
	Version    string
	Installed  bool
	Fields     []Field
	Requires   []string // direct dependencies
	RequiredBy []string // reverse direct dependencies, when the manager knows them
	Source     string   // command or URL the data came from
	Raw        string   // verbatim output of the underlying tool
}

// Command is a single external command to execute.
type Command struct {
	Name string
	Args []string
	Env  []string // extra KEY=VALUE entries layered on top of the process env
}

// Display renders the command the way a shell user would have typed it.
func (c Command) Display() string {
	parts := make([]string, 0, len(c.Args)+1)
	parts = append(parts, c.Name)
	for _, a := range c.Args {
		if a == "" || strings.ContainsAny(a, " \t\"'$*?") {
			parts = append(parts, fmt.Sprintf("%q", a))
			continue
		}
		parts = append(parts, a)
	}
	return strings.Join(parts, " ")
}

// Plan is an ordered list of commands that together perform one operation.
// Steps run in order and stop at the first failure.
type Plan struct {
	Title string
	Steps []Command
}

// Extra is a manager specific maintenance action surfaced in the UI.
type Extra struct {
	Key   string // key binding label
	Title string
	Plan  Plan
}

// Manager is the common surface of a package manager.
type Manager interface {
	// ID is the short identifier, e.g. "pip".
	ID() string
	// Label is the tab label, e.g. "pip (PyPI)".
	Label() string
	// Kind describes what the manager manages, for the header.
	Kind() string
	// Probe reports whether the manager is usable on this host.
	Probe(ctx context.Context) error

	// List returns the packages currently installed.
	List(ctx context.Context) ([]Package, error)
	// Search looks for packages by name or keyword, installed or not.
	Search(ctx context.Context, query string) ([]Package, error)
	// Details expands a single package.
	Details(ctx context.Context, p Package) (*Details, error)
	// Outdated maps package name to the newer version available.
	Outdated(ctx context.Context) (map[string]string, error)

	// InstallPlan installs spec, which may carry a version constraint.
	InstallPlan(ctx context.Context, spec string) (Plan, error)
	// UpgradePlan upgrades an installed package to the newest version.
	UpgradePlan(ctx context.Context, name string) (Plan, error)
	// RemovePlan uninstalls a package.
	RemovePlan(ctx context.Context, name string) (Plan, error)

	// Extras lists optional maintenance actions.
	Extras() []Extra
	// SpecHint is placeholder text for the install prompt.
	SpecHint() string
}

// OrphanFinder is implemented by managers that can work out which dependencies
// are no longer needed once a package has been removed.
type OrphanFinder interface {
	// Snapshot records the dependency graph before a removal.
	Snapshot(ctx context.Context) (Snapshot, error)
	// Orphans returns still-installed packages that only existed to satisfy
	// removed and that nothing else depends on any more.
	Orphans(ctx context.Context, before Snapshot, removed []string) ([]string, error)
	// RemovePlanMany uninstalls several packages in one go.
	RemovePlanMany(ctx context.Context, names []string) (Plan, error)
}

// Snapshot is an opaque record of a dependency graph at a point in time.
type Snapshot map[string]node

type node struct {
	Version  string   `json:"version"`
	Requires []string `json:"requires"`
	Summary  string   `json:"summary"`
}

// Logger receives a running commentary of everything the managers do, so the
// UI can show the real commands and their output.
type Logger func(line string)

func (l Logger) log(format string, args ...any) {
	if l == nil {
		return
	}
	l(fmt.Sprintf(format, args...))
}

// sortPackages orders packages case-insensitively by name.
func sortPackages(pkgs []Package) {
	sort.Slice(pkgs, func(i, j int) bool {
		a, b := strings.ToLower(pkgs[i].Name), strings.ToLower(pkgs[j].Name)
		if a == b {
			return pkgs[i].Name < pkgs[j].Name
		}
		return a < b
	})
}

// stableSortBy sorts pkgs with a less function over values rather than indices.
func stableSortBy(pkgs []Package, less func(a, b Package) bool) {
	sort.SliceStable(pkgs, func(i, j int) bool { return less(pkgs[i], pkgs[j]) })
}
