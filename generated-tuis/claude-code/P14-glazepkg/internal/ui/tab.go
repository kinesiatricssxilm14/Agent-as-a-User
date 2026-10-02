package ui

import (
	"sort"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"

	"tooln/internal/pkgmgr"
)

// sourceKind says where the rows in a tab came from.
type sourceKind int

const (
	sourceInstalled sourceKind = iota // the live environment
	sourceSearch                      // a query against the package index
)

// probeState tracks whether a manager has been found usable yet. It has to be
// three-valued: "not answered yet" must not be treated as "unusable", or the
// first manager to finish probing would steal the active tab from the one the
// user is meant to land on.
type probeState int

const (
	probePending probeState = iota
	probeOK
	probeFailed
)

// tabState is everything one package manager view remembers. Each manager keeps
// its own cursor, marks, filter and details cache, so switching tabs never
// loses your place.
type tabState struct {
	mgr pkgmgr.Manager

	probe    probeState
	probeErr string // why the manager is unusable
	loaded   bool   // List has completed at least once
	listing  bool   // a List or Search is in flight for this manager

	all    []pkgmgr.Package
	source sourceKind
	query  string // the search that produced `all`, when source is sourceSearch

	filter        string
	installedOnly bool

	rows   []int // indices into all, after filtering
	cursor int   // index into rows
	top    int   // first visible row, for scrolling

	marks map[string]bool

	details    map[string]*pkgmgr.Details
	detailErrs map[string]string
	pending    map[string]bool // details lookups in flight

	outdated    map[string]string
	outdatedErr string
	checked     bool // Outdated has run since the last refresh

	vp        viewport.Model
	shownName string // package whose details are currently rendered
	loadErr   string
}

func newTabState(m pkgmgr.Manager) *tabState {
	vp := viewport.New(40, 10)
	vp.MouseWheelEnabled = false
	return &tabState{
		mgr:        m,
		marks:      map[string]bool{},
		details:    map[string]*pkgmgr.Details{},
		detailErrs: map[string]string{},
		pending:    map[string]bool{},
		outdated:   map[string]string{},
		vp:         vp,
	}
}

// available reports whether the manager can be used.
func (t *tabState) available() bool { return t.probe == probeOK }

// visible returns the packages currently listed, in display order.
func (t *tabState) visible() []pkgmgr.Package {
	out := make([]pkgmgr.Package, 0, len(t.rows))
	for _, i := range t.rows {
		out = append(out, t.all[i])
	}
	return out
}

// current returns the package under the cursor.
func (t *tabState) current() (pkgmgr.Package, bool) {
	if t.cursor < 0 || t.cursor >= len(t.rows) {
		return pkgmgr.Package{}, false
	}
	return t.all[t.rows[t.cursor]], true
}

// setPackages replaces the rows and keeps the cursor on the same package where
// possible, so a refresh does not throw away the user's position.
func (t *tabState) setPackages(pkgs []pkgmgr.Package, src sourceKind, query string) {
	var keep string
	if cur, ok := t.current(); ok {
		keep = cur.Name
	}
	t.all = pkgs
	t.source = src
	t.query = query
	t.loaded = true
	t.loadErr = ""
	t.applyFilter()
	if keep != "" {
		t.selectByName(keep)
	}
	t.pruneMarks()
}

// applyFilter recomputes the visible rows from the filter text and the
// installed-only toggle, then clamps the cursor.
func (t *tabState) applyFilter() {
	needle := strings.ToLower(strings.TrimSpace(t.filter))
	t.rows = t.rows[:0]
	for i, p := range t.all {
		if t.installedOnly && !p.Installed {
			continue
		}
		if needle != "" && !matches(p, needle) {
			continue
		}
		t.rows = append(t.rows, i)
	}
	t.clampCursor()
}

// matches reports whether a package satisfies the filter. Name matching ignores
// the difference between "-", "_" and "." so typing either spelling works.
func matches(p pkgmgr.Package, needle string) bool {
	if strings.Contains(strings.ToLower(p.Name), needle) {
		return true
	}
	if strings.Contains(loosen(p.Name), loosen(needle)) {
		return true
	}
	if strings.Contains(strings.ToLower(p.Summary), needle) {
		return true
	}
	return strings.Contains(strings.ToLower(p.Version), needle)
}

func loosen(s string) string {
	r := strings.NewReplacer("-", "", "_", "", ".", "")
	return r.Replace(strings.ToLower(s))
}

func (t *tabState) clampCursor() {
	if len(t.rows) == 0 {
		t.cursor, t.top = 0, 0
		return
	}
	if t.cursor >= len(t.rows) {
		t.cursor = len(t.rows) - 1
	}
	if t.cursor < 0 {
		t.cursor = 0
	}
}

// selectByName moves the cursor to a package by name, matching the way the
// package managers normalise names. It reports whether the package was found.
func (t *tabState) selectByName(name string) bool {
	target := loosen(name)
	for i, row := range t.rows {
		if loosen(t.all[row].Name) == target {
			t.cursor = i
			return true
		}
	}
	// The package may exist but be hidden by the current filter; drop the
	// filter rather than silently leaving the cursor elsewhere.
	for _, p := range t.all {
		if loosen(p.Name) == target {
			t.filter = ""
			t.installedOnly = false
			t.applyFilter()
			for i, row := range t.rows {
				if loosen(t.all[row].Name) == target {
					t.cursor = i
					return true
				}
			}
			break
		}
	}
	return false
}

func (t *tabState) moveCursor(delta int) {
	if len(t.rows) == 0 {
		return
	}
	t.cursor += delta
	if t.cursor < 0 {
		t.cursor = 0
	}
	if t.cursor >= len(t.rows) {
		t.cursor = len(t.rows) - 1
	}
}

// ensureVisible scrolls the window so the cursor sits inside it.
func (t *tabState) ensureVisible(height int) {
	if height <= 0 {
		t.top = t.cursor
		return
	}
	if t.cursor < t.top {
		t.top = t.cursor
	}
	if t.cursor >= t.top+height {
		t.top = t.cursor - height + 1
	}
	maxTop := max(0, len(t.rows)-height)
	if t.top > maxTop {
		t.top = maxTop
	}
	if t.top < 0 {
		t.top = 0
	}
}

// markedNames returns the marked packages that are still present, in display
// order so operations run in a predictable sequence.
func (t *tabState) markedNames() []string {
	var out []string
	for _, i := range t.rows {
		if t.marks[t.all[i].Name] {
			out = append(out, t.all[i].Name)
		}
	}
	// Include marks hidden by the current filter, so a filter change cannot
	// quietly drop a package the user asked to act on.
	seen := map[string]bool{}
	for _, n := range out {
		seen[n] = true
	}
	var hidden []string
	for name := range t.marks {
		if !seen[name] && t.hasPackage(name) {
			hidden = append(hidden, name)
		}
	}
	sort.Strings(hidden)
	return append(out, hidden...)
}

func (t *tabState) hasPackage(name string) bool {
	_, ok := t.packageByName(name)
	return ok
}

// packageByName looks a package up in the current rows by exact name.
func (t *tabState) packageByName(name string) (pkgmgr.Package, bool) {
	for _, p := range t.all {
		if p.Name == name {
			return p, true
		}
	}
	return pkgmgr.Package{}, false
}

// pruneMarks forgets marks for packages that no longer exist.
func (t *tabState) pruneMarks() {
	for name := range t.marks {
		if !t.hasPackage(name) {
			delete(t.marks, name)
		}
	}
}

// invalidate drops cached details and update information after the environment
// has changed, so nothing stale can be displayed.
func (t *tabState) invalidate() {
	t.details = map[string]*pkgmgr.Details{}
	t.detailErrs = map[string]string{}
	t.pending = map[string]bool{}
	t.outdated = map[string]string{}
	t.outdatedErr = ""
	t.checked = false
	t.shownName = ""
}

// countInstalled reports how many of the visible rows are installed.
func (t *tabState) countInstalled() int {
	n := 0
	for _, i := range t.rows {
		if t.all[i].Installed {
			n++
		}
	}
	return n
}
