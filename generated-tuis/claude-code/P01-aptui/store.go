package main

import (
	"sort"
	"strings"
)

// pkg is toola's unified view of one package, merged from every source: the
// dpkg database (what is installed), the apt available index (what could be
// installed), and apt's own upgrade plan (what would change).
type pkg struct {
	Name     string
	Section  string
	Synopsis string

	Installed        bool
	InstalledVersion string
	Status           string // raw dpkg state, for packages that are not "installed"

	Available        bool
	AvailableVersion string

	Upgradable    bool
	UpgradeTarget string
}

// DisplayVersion is the version most relevant to the user: what they have if
// they have it, otherwise what they would get.
func (p pkg) DisplayVersion() string {
	if p.Installed && p.InstalledVersion != "" {
		return p.InstalledVersion
	}
	return p.AvailableVersion
}

// ResidualConfig reports whether dpkg knows the package but it is not
// installed, e.g. it was removed without purging and its conffiles remain.
func (p pkg) ResidualConfig() bool {
	return !p.Installed && p.Status != "" && p.Status != "not-installed"
}

// filterMode selects which subset of packages the list shows.
type filterMode int

const (
	filterAll filterMode = iota
	filterInstalled
	filterAvailable
	filterUpgradable
	filterResidual
)

func (f filterMode) String() string {
	switch f {
	case filterInstalled:
		return "installed"
	case filterAvailable:
		return "not installed"
	case filterUpgradable:
		return "upgradable"
	case filterResidual:
		return "residual config"
	default:
		return "all"
	}
}

// match reports whether p belongs in this filter.
func (f filterMode) match(p pkg) bool {
	switch f {
	case filterInstalled:
		return p.Installed
	case filterAvailable:
		return p.Available && !p.Installed
	case filterUpgradable:
		return p.Upgradable
	case filterResidual:
		return p.ResidualConfig()
	default:
		return true
	}
}

// store holds every known package and the derived, ordered view that the list
// renders. Rebuild recomputes the view; nothing else mutates it.
type store struct {
	byName map[string]*pkg
	names  []string // every known name, sorted

	view []*pkg // current filter+query result, in display order

	query  string
	filter filterMode

	// counts describe the whole store, independent of the active filter, so
	// the header can always show the totals.
	countInstalled  int
	countAvailable  int
	countUpgradable int
	countResidual   int
}

func newStore() *store {
	return &store{byName: make(map[string]*pkg)}
}

// get returns the entry for name, creating it if this is the first source to
// mention it. Callers must call rebuild once they finish mutating.
func (s *store) get(name string) *pkg {
	if p, ok := s.byName[name]; ok {
		return p
	}
	p := &pkg{Name: name}
	s.byName[name] = p
	s.names = append(s.names, name)
	return p
}

// lookup returns the entry for name without creating it.
func (s *store) lookup(name string) *pkg {
	return s.byName[name]
}

// setInstalled replaces the installed state of every package from a fresh
// dpkg-query result. Packages that disappeared from dpkg's database are marked
// not installed rather than deleted, because they may still be available.
func (s *store) setInstalled(pkgs []installedPkg) {
	for _, p := range s.byName {
		p.Installed = false
		p.InstalledVersion = ""
		p.Status = ""
	}
	for _, in := range pkgs {
		p := s.get(in.Name)
		p.Status = in.Status
		if in.FullyInstalled() {
			p.Installed = true
			p.InstalledVersion = in.Version
		}
	}
}

// setAvailable replaces the available index from a fresh dumpavail result.
func (s *store) setAvailable(pkgs []availablePkg) {
	for _, p := range s.byName {
		p.Available = false
		p.AvailableVersion = ""
	}
	for _, av := range pkgs {
		p := s.get(av.Name)
		p.Available = true
		p.AvailableVersion = av.Version
		if av.Section != "" {
			p.Section = av.Section
		}
		if av.Synopsis != "" {
			p.Synopsis = av.Synopsis
		}
	}
}

// setUpgradable replaces the upgradable set from a fresh `apt-get -s upgrade`
// plan. Entries that apt lists as installs rather than upgrades (no previous
// version) are skipped: they are new dependencies pulled in by the upgrade, not
// packages the user can choose to upgrade.
func (s *store) setUpgradable(cands []upgradeCandidate) {
	for _, p := range s.byName {
		p.Upgradable = false
		p.UpgradeTarget = ""
	}
	for _, c := range cands {
		// Every package apt names really exists, so record it. Entries with no
		// previous version are new dependencies the upgrade would pull in, not
		// packages the user can choose to upgrade, so they stay unmarked.
		p := s.get(c.Name)
		if c.OldVersion == "" {
			continue
		}
		p.Upgradable = true
		p.UpgradeTarget = c.NewVersion
	}
}

// applyPolicy fills in candidate versions for packages that dumpavail did not
// cover. It never clears state, so it is safe to run after any other source.
func (s *store) applyPolicy(entries []policyEntry) {
	for _, e := range entries {
		if e.Candidate == "" {
			continue
		}
		p := s.get(e.Name)
		if p.AvailableVersion == "" {
			p.AvailableVersion = e.Candidate
			p.Available = true
		}
	}
}

// setSynopsis records a synopsis learned from a details fetch, so that packages
// which are installed but absent from every repository still describe
// themselves in the list.
func (s *store) setSynopsis(name, section, synopsis string) {
	p, ok := s.byName[name]
	if !ok {
		return
	}
	if p.Synopsis == "" && synopsis != "" {
		p.Synopsis = synopsis
	}
	if p.Section == "" && section != "" {
		p.Section = section
	}
}

// rebuild recomputes counts and the visible view. It must be called after any
// mutation of the store.
func (s *store) rebuild() {
	sort.Strings(s.names)

	s.countInstalled, s.countAvailable, s.countUpgradable, s.countResidual = 0, 0, 0, 0
	for _, name := range s.names {
		p := s.byName[name]
		if p.Installed {
			s.countInstalled++
		}
		if p.Available {
			s.countAvailable++
		}
		if p.Upgradable {
			s.countUpgradable++
		}
		if p.ResidualConfig() {
			s.countResidual++
		}
	}

	query := strings.ToLower(strings.TrimSpace(s.query))
	s.view = s.view[:0]

	type scored struct {
		p    *pkg
		rank int
	}
	var matches []scored

	for _, name := range s.names {
		p := s.byName[name]
		if !s.filter.match(*p) {
			continue
		}
		rank, ok := matchRank(*p, query)
		if !ok {
			continue
		}
		matches = append(matches, scored{p: p, rank: rank})
	}

	// Rank first, then name, so exact and prefix matches surface at the top
	// while the ordering stays stable and predictable.
	sort.SliceStable(matches, func(i, j int) bool {
		if matches[i].rank != matches[j].rank {
			return matches[i].rank < matches[j].rank
		}
		return matches[i].p.Name < matches[j].p.Name
	})

	for _, m := range matches {
		s.view = append(s.view, m.p)
	}
}

// Match ranks. Lower sorts earlier.
const (
	rankExact       = 0
	rankPrefix      = 1
	rankSubstring   = 2
	rankDescription = 3
)

// matchRank scores a package against a lower-cased query, reporting whether it
// matches at all. An empty query matches everything at equal rank so the list
// stays alphabetical.
func matchRank(p pkg, query string) (int, bool) {
	if query == "" {
		return rankExact, true
	}

	name := strings.ToLower(p.Name)
	switch {
	case name == query:
		return rankExact, true
	case strings.HasPrefix(name, query):
		return rankPrefix, true
	case strings.Contains(name, query):
		return rankSubstring, true
	}

	// Fall back to the synopsis so that searching for a concept ("http
	// server") finds packages whose names do not contain the words.
	if strings.Contains(strings.ToLower(p.Synopsis), query) {
		return rankDescription, true
	}
	return 0, false
}

// visible returns the current filtered, searched, ordered slice.
func (s *store) visible() []*pkg {
	return s.view
}

// installedNames returns every package dpkg knows about, for the bulk
// apt-cache policy call.
func (s *store) installedNames() []string {
	var names []string
	for _, name := range s.names {
		if s.byName[name].Installed {
			names = append(names, name)
		}
	}
	return names
}
