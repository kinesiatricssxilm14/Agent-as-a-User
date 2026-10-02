package pkgmgr

import (
	"context"
	"errors"
	"fmt"
	"os"
	"regexp"
	"strings"
)

// Apt drives Debian's package manager. Reads go through dpkg-query and
// apt-cache (fast, no lock needed); writes go through apt-get.
type Apt struct {
	log Logger

	dpkgQuery string
	aptCache  string
	aptGet    string
	aptBin    string // the `apt` wrapper, used for `apt list --upgradable`
	probed    bool
}

func NewApt(log Logger) *Apt { return &Apt{log: log} }

func (a *Apt) ID() string    { return "apt" }
func (a *Apt) Label() string { return "apt" }
func (a *Apt) Kind() string  { return "Debian system packages" }

func (a *Apt) SpecHint() string { return "package  or  package=1.2-3  (dpkg name)" }

// Probe checks that the dpkg/apt tools this manager needs are present.
func (a *Apt) Probe(ctx context.Context) error {
	if a.probed {
		return nil
	}
	var missing []string
	for _, t := range []struct {
		name string
		dst  *string
		need bool
	}{
		{"dpkg-query", &a.dpkgQuery, true},
		{"apt-cache", &a.aptCache, true},
		{"apt-get", &a.aptGet, true},
		{"apt", &a.aptBin, false},
	} {
		path, ok := lookPath(t.name)
		if !ok {
			if t.need {
				missing = append(missing, t.name)
			}
			continue
		}
		*t.dst = path
	}
	if len(missing) > 0 {
		return fmt.Errorf("missing required tools: %s", strings.Join(missing, ", "))
	}
	a.probed = true
	return nil
}

// dpkgListFormat asks dpkg-query for exactly the fields the list needs, one
// record per line, with a separator that cannot appear in any of them.
const dpkgListFormat = "${db:Status-Abbrev}\t${binary:Package}\t${Version}\t${Architecture}\t${binary:Summary}\n"

// List returns the packages dpkg reports as fully installed. Packages left in
// a config-files or half-installed state are excluded: they are not usable, and
// showing them would misrepresent the environment.
func (a *Apt) List(ctx context.Context) ([]Package, error) {
	out, err := run(ctx, a.log, Command{
		Name: a.dpkgQuery,
		Args: []string{"--show", "--showformat=" + dpkgListFormat},
	})
	if err != nil && strings.TrimSpace(out) == "" {
		return nil, err
	}
	var pkgs []Package
	for _, line := range strings.Split(out, "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		fields := strings.SplitN(line, "\t", 5)
		if len(fields) < 3 {
			continue
		}
		status := strings.TrimSpace(fields[0])
		// "ii" means desired=install, current=installed.
		if !strings.HasPrefix(status, "ii") {
			continue
		}
		pkg := Package{
			Name:      strings.TrimSpace(fields[1]),
			Version:   strings.TrimSpace(fields[2]),
			Installed: true,
		}
		if len(fields) >= 4 {
			if arch := strings.TrimSpace(fields[3]); arch != "" {
				pkg.Note = arch
			}
		}
		if len(fields) >= 5 {
			pkg.Summary = strings.TrimSpace(fields[4])
		}
		if pkg.Name != "" {
			pkgs = append(pkgs, pkg)
		}
	}
	sortPackages(pkgs)
	return pkgs, nil
}

var upgradableRe = regexp.MustCompile(`^([^/\s]+)/\S+\s+(\S+)\s`)

// Outdated parses `apt list --upgradable` for packages with a newer candidate.
func (a *Apt) Outdated(ctx context.Context) (map[string]string, error) {
	if a.aptBin == "" {
		return nil, errors.New("the apt command is not available; cannot list upgradable packages")
	}
	out, err := run(ctx, a.log, Command{Name: a.aptBin, Args: []string{"list", "--upgradable"}})
	if err != nil {
		return nil, err
	}
	res := map[string]string{}
	for _, line := range strings.Split(out, "\n") {
		m := upgradableRe.FindStringSubmatch(strings.TrimSpace(line))
		if m == nil {
			continue
		}
		name := m[1]
		if i := strings.IndexByte(name, ':'); i >= 0 { // drop :arch qualifier
			name = name[:i]
		}
		res[name] = m[2]
	}
	return res, nil
}

// Search uses apt-cache's own name/description search over the package index.
func (a *Apt) Search(ctx context.Context, query string) ([]Package, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, errors.New("empty search query")
	}
	if err := validateAptQuery(query); err != nil {
		return nil, err
	}

	out, err := run(ctx, a.log, Command{
		Name: a.aptCache,
		Args: []string{"search", "--names-only", query},
	})
	if err != nil {
		return nil, err
	}
	// apt-cache search prints "name - summary".
	var found []Package
	for _, line := range strings.Split(out, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		name, summary := line, ""
		if i := strings.Index(line, " - "); i > 0 {
			name, summary = strings.TrimSpace(line[:i]), strings.TrimSpace(line[i+3:])
		}
		found = append(found, Package{Name: name, Summary: summary})
	}
	if len(found) == 0 {
		// Fall back to a full-text search before giving up.
		out, err = run(ctx, a.log, Command{Name: a.aptCache, Args: []string{"search", query}})
		if err == nil {
			for _, line := range strings.Split(out, "\n") {
				if i := strings.Index(line, " - "); i > 0 {
					found = append(found, Package{
						Name:    strings.TrimSpace(line[:i]),
						Summary: strings.TrimSpace(line[i+3:]),
					})
				}
			}
		}
	}
	if len(found) == 0 {
		return nil, fmt.Errorf("no apt packages match %q", query)
	}

	// Annotate with installed and candidate versions from the real dpkg state.
	installed, err := a.List(ctx)
	if err != nil {
		a.log.log("! could not read dpkg state: %v", err)
	}
	byName := make(map[string]Package, len(installed))
	for _, p := range installed {
		byName[p.Name] = p
	}
	const versionLookupLimit = 60 // apt-cache policy on hundreds of names is slow
	policy := map[string]aptPolicy{}
	if len(found) <= versionLookupLimit {
		policy = a.policy(ctx, packageNames(found))
	}
	for i := range found {
		if local, ok := byName[found[i].Name]; ok {
			found[i].Installed = true
			found[i].Version = local.Version
			found[i].Note = local.Note
			if pol, ok := policy[found[i].Name]; ok && pol.Candidate != "" &&
				pol.Candidate != local.Version {
				found[i].Latest = pol.Candidate
			}
			continue
		}
		if pol, ok := policy[found[i].Name]; ok {
			found[i].Version = pol.Candidate
		}
	}
	sortSearchResults(found, query)
	return found, nil
}

type aptPolicy struct {
	Installed string
	Candidate string
}

// policy reads installed and candidate versions straight from apt-cache policy.
func (a *Apt) policy(ctx context.Context, pkgNames []string) map[string]aptPolicy {
	res := map[string]aptPolicy{}
	if len(pkgNames) == 0 {
		return res
	}
	args := append([]string{"policy"}, pkgNames...)
	out, err := run(ctx, nil, Command{Name: a.aptCache, Args: args})
	if err != nil && strings.TrimSpace(out) == "" {
		return res
	}
	var cur string
	for _, raw := range strings.Split(out, "\n") {
		line := strings.TrimRight(raw, "\r")
		trimmed := strings.TrimSpace(line)
		switch {
		case trimmed == "":
		case !strings.HasPrefix(line, " ") && strings.HasSuffix(trimmed, ":"):
			cur = strings.TrimSuffix(trimmed, ":")
			res[cur] = aptPolicy{}
		case cur == "":
		case strings.HasPrefix(trimmed, "Installed:"):
			p := res[cur]
			p.Installed = cleanAptVersion(strings.TrimPrefix(trimmed, "Installed:"))
			res[cur] = p
		case strings.HasPrefix(trimmed, "Candidate:"):
			p := res[cur]
			p.Candidate = cleanAptVersion(strings.TrimPrefix(trimmed, "Candidate:"))
			res[cur] = p
		}
	}
	return res
}

func cleanAptVersion(s string) string {
	s = strings.TrimSpace(s)
	if s == "(none)" {
		return ""
	}
	return s
}

// Details combines dpkg's installed metadata with apt-cache's index record, so
// the pane shows both what is on disk and what the archive offers.
func (a *Apt) Details(ctx context.Context, pkg Package) (*Details, error) {
	if err := validateAptName(pkg.Name); err != nil {
		return nil, err
	}

	d := &Details{Name: pkg.Name, Version: pkg.Version, Installed: pkg.Installed}
	var raw strings.Builder

	add := func(key, value string) {
		if v := strings.TrimSpace(value); v != "" {
			d.Fields = append(d.Fields, Field{Key: key, Value: v})
		}
	}

	pol := a.policy(ctx, []string{pkg.Name})[pkg.Name]

	// apt-cache show gives the archive's record for the candidate version.
	showOut, showErr := run(ctx, a.log, Command{
		Name: a.aptCache, Args: []string{"show", pkg.Name},
	})
	var b block
	if blocks := parseRFC822(showOut); len(blocks) > 0 {
		b = blocks[0]
	} else {
		b = newBlock()
	}

	// dpkg-query is authoritative for what is actually installed.
	var installedVersion string
	if out, err := run(ctx, a.log, Command{
		Name: a.dpkgQuery,
		Args: []string{"--show", "--showformat=${db:Status-Abbrev}\t${Version}\n", pkg.Name},
	}); err == nil {
		fields := strings.SplitN(strings.TrimSpace(out), "\t", 2)
		if len(fields) == 2 && strings.HasPrefix(strings.TrimSpace(fields[0]), "ii") {
			installedVersion = strings.TrimSpace(fields[1])
			d.Installed = true
		}
	}
	if installedVersion == "" && pol.Installed != "" {
		installedVersion = pol.Installed
		d.Installed = true
	}
	if installedVersion != "" {
		d.Version = installedVersion
	} else if d.Version == "" {
		d.Version = pol.Candidate
	}

	add("Package", pkg.Name)
	if installedVersion != "" {
		add("Installed version", installedVersion)
		add("Status", "installed")
	} else {
		add("Status", "not installed")
	}
	if pol.Candidate != "" {
		add("Candidate version", pol.Candidate)
		if installedVersion != "" && pol.Candidate != installedVersion {
			d.Fields = append(d.Fields, Field{Key: "Upgrade available", Value: pol.Candidate})
		}
	}
	add("Architecture", b.get("Architecture"))
	add("Section", b.get("Section"))
	add("Priority", b.get("Priority"))
	add("Maintainer", b.get("Maintainer"))
	add("Installed-Size", humanKB(b.get("Installed-Size")))
	add("Homepage", b.get("Homepage"))
	add("Source", b.get("Source"))

	d.Requires = aptDepNames(b.get("Depends"))
	add("Depends", orNone(strings.Join(splitList2(b.get("Depends")), ", ")))
	add("Pre-Depends", strings.Join(splitList2(b.get("Pre-Depends")), ", "))
	add("Recommends", strings.Join(splitList2(b.get("Recommends")), ", "))
	add("Suggests", strings.Join(splitList2(b.get("Suggests")), ", "))
	add("Conflicts", strings.Join(splitList2(b.get("Conflicts")), ", "))
	add("Provides", strings.Join(splitList2(b.get("Provides")), ", "))

	// Reverse dependencies limited to what is actually installed here.
	if rdeps, err := a.reverseDepends(ctx, pkg.Name); err == nil {
		d.RequiredBy = rdeps
		add("Required by (installed)", orNone(strings.Join(rdeps, ", ")))
	}
	add("Description", b.get("Description"))
	if summary := firstNonEmpty(pkg.Summary, b.get("Description-en")); summary != "" && b.get("Description") == "" {
		add("Description", summary)
	}

	raw.WriteString(strings.TrimSpace(showOut))
	d.Raw = raw.String()
	d.Source = "dpkg-query + apt-cache show " + pkg.Name

	if len(d.Fields) <= 2 && showErr != nil {
		return nil, fmt.Errorf("no information for %q: %w", pkg.Name, showErr)
	}
	return d, nil
}

// reverseDepends returns installed packages that depend on name.
func (a *Apt) reverseDepends(ctx context.Context, name string) ([]string, error) {
	out, err := run(ctx, nil, Command{
		Name: a.aptCache,
		Args: []string{"--installed", "rdepends", name},
	})
	if err != nil {
		return nil, err
	}
	installed, err := a.List(ctx)
	if err != nil {
		return nil, err
	}
	live := make(map[string]bool, len(installed))
	for _, p := range installed {
		live[p.Name] = true
	}
	seen := map[string]bool{name: true}
	var out2 []string
	for _, line := range strings.Split(out, "\n") {
		dep := strings.TrimSpace(line)
		dep = strings.TrimPrefix(dep, "|")
		dep = strings.TrimSpace(dep)
		if dep == "" || strings.HasSuffix(dep, ":") || strings.HasPrefix(dep, "Reverse Depends") {
			continue
		}
		if i := strings.IndexAny(dep, " ("); i > 0 {
			dep = dep[:i]
		}
		if seen[dep] || !live[dep] {
			continue
		}
		seen[dep] = true
		out2 = append(out2, dep)
	}
	return out2, nil
}

// ------------------------------------------------------------------ plans ----

// InstallPlan installs spec with apt-get, refreshing the index first when the
// package lists are absent (Debian slim images ship with them stripped, and
// apt-get install cannot resolve anything until they are fetched).
func (a *Apt) InstallPlan(ctx context.Context, spec string) (Plan, error) {
	spec = strings.TrimSpace(spec)
	if spec == "" {
		return Plan{}, errors.New("no package specified")
	}
	if err := validateAptName(spec); err != nil {
		return Plan{}, err
	}
	plan := Plan{Title: "apt-get install " + spec}
	if !hasPackageLists() {
		plan.Steps = append(plan.Steps, Command{Name: a.aptGet, Args: []string{"update"}})
	}
	plan.Steps = append(plan.Steps, Command{
		Name: a.aptGet,
		Args: []string{"install", "-y", "--no-install-recommends", spec},
	})
	return plan, nil
}

// hasPackageLists reports whether apt has any package index on disk. This is a
// plain directory read rather than a subprocess, so it is safe to call while
// building a plan.
func hasPackageLists() bool {
	entries, err := os.ReadDir("/var/lib/apt/lists")
	if err != nil {
		return false
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		// Fetched indexes are named like
		// deb.debian.org_debian_dists_bookworm_main_binary-amd64_Packages.
		if strings.HasSuffix(e.Name(), "_Packages") ||
			strings.Contains(e.Name(), "_Packages.") {
			return true
		}
	}
	return false
}

func (a *Apt) UpgradePlan(ctx context.Context, name string) (Plan, error) {
	name = strings.TrimSpace(name)
	if err := validateAptName(name); err != nil {
		return Plan{}, err
	}
	return Plan{
		Title: "apt-get install --only-upgrade " + name,
		Steps: []Command{
			{Name: a.aptGet, Args: []string{"update"}},
			{Name: a.aptGet, Args: []string{"install", "-y", "--only-upgrade", name}},
		},
	}, nil
}

func (a *Apt) RemovePlan(ctx context.Context, name string) (Plan, error) {
	return a.RemovePlanMany(ctx, []string{name})
}

// RemovePlanMany removes packages and then drops dependencies that were only
// pulled in automatically, so the list reflects a genuinely clean removal.
func (a *Apt) RemovePlanMany(ctx context.Context, names []string) (Plan, error) {
	clean := make([]string, 0, len(names))
	for _, n := range names {
		n = strings.TrimSpace(n)
		if n == "" {
			continue
		}
		if err := validateAptName(n); err != nil {
			return Plan{}, err
		}
		clean = append(clean, n)
	}
	if len(clean) == 0 {
		return Plan{}, errors.New("no package specified")
	}
	args := append([]string{"remove", "-y", "--auto-remove"}, clean...)
	return Plan{
		Title: "apt-get remove --auto-remove " + strings.Join(clean, " "),
		Steps: []Command{{Name: a.aptGet, Args: args}},
	}, nil
}

func (a *Apt) Extras() []Extra {
	return []Extra{
		{Key: "u", Title: "apt-get update (refresh package lists)", Plan: Plan{
			Title: "apt-get update",
			Steps: []Command{{Name: a.aptGet, Args: []string{"update"}}},
		}},
		{Key: "c", Title: "apt-get autoremove (drop unused dependencies)", Plan: Plan{
			Title: "apt-get autoremove",
			Steps: []Command{{Name: a.aptGet, Args: []string{"autoremove", "-y"}}},
		}},
		{Key: "C", Title: "apt-get clean (free the download cache)", Plan: Plan{
			Title: "apt-get clean",
			Steps: []Command{{Name: a.aptGet, Args: []string{"clean"}}},
		}},
	}
}

// ------------------------------------------------------------------ utils ----

// aptNamePattern matches a dpkg package name, optionally with an :arch
// qualifier and an =version pin.
var aptNamePattern = regexp.MustCompile(`^[a-zA-Z0-9][a-zA-Z0-9+.\-]*(:[a-zA-Z0-9\-]+)?(=[A-Za-z0-9.+:~\-]+)?$`)

func validateAptName(name string) error {
	name = strings.TrimSpace(name)
	if name == "" {
		return errors.New("no package specified")
	}
	if !aptNamePattern.MatchString(name) {
		return fmt.Errorf("%q is not a valid apt package name", name)
	}
	return nil
}

// validateAptQuery allows regular expressions (apt-cache search takes them)
// while refusing shell metacharacters.
var aptQueryPattern = regexp.MustCompile(`^[a-zA-Z0-9+.\-_^$*?()\[\]{}|\\/: ]+$`)

func validateAptQuery(q string) error {
	if !aptQueryPattern.MatchString(q) {
		return fmt.Errorf("%q contains characters that are not allowed in a search", q)
	}
	return nil
}

// splitList2 parses a Debian relationship field into its alternatives, keeping
// version constraints intact for display.
func splitList2(s string) []string {
	s = strings.TrimSpace(s)
	if s == "" {
		return nil
	}
	var out []string
	for _, part := range strings.Split(s, ",") {
		if p := strings.Join(strings.Fields(part), " "); p != "" {
			out = append(out, p)
		}
	}
	return out
}

// aptDepNames reduces a Depends field to bare package names, taking the first
// alternative of each "a | b" group.
func aptDepNames(depends string) []string {
	var out []string
	seen := map[string]bool{}
	for _, group := range splitList2(depends) {
		first := strings.TrimSpace(strings.Split(group, "|")[0])
		if i := strings.IndexAny(first, " ("); i > 0 {
			first = first[:i]
		}
		if first != "" && !seen[first] {
			seen[first] = true
			out = append(out, first)
		}
	}
	return out
}

func humanKB(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return ""
	}
	var kb float64
	if _, err := fmt.Sscanf(s, "%f", &kb); err != nil {
		return s
	}
	if kb >= 1024 {
		return fmt.Sprintf("%.1f MB", kb/1024)
	}
	return fmt.Sprintf("%.0f kB", kb)
}

func packageNames(pkgs []Package) []string {
	out := make([]string, 0, len(pkgs))
	for _, p := range pkgs {
		out = append(out, p.Name)
	}
	return out
}

func sortSearchResults(pkgs []Package, query string) {
	q := strings.ToLower(query)
	rank := func(p Package) int {
		name := strings.ToLower(p.Name)
		switch {
		case name == q:
			return 0
		case strings.HasPrefix(name, q):
			return 1
		case p.Installed:
			return 2
		default:
			return 3
		}
	}
	stableSortBy(pkgs, func(a, b Package) bool {
		ra, rb := rank(a), rank(b)
		if ra != rb {
			return ra < rb
		}
		return strings.ToLower(a.Name) < strings.ToLower(b.Name)
	})
}
