package pkgmgr

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"sort"
	"strings"
)

// Pip drives the Python package manager through `<python> -m pip`.
type Pip struct {
	log Logger

	python     string   // interpreter path, e.g. /usr/bin/python3
	pipArgs    []string // argv prefix, e.g. ["-m", "pip"]
	extraFlags []string // flags pip needs on this host (e.g. --break-system-packages)
	env        []string
	probed     bool
}

// NewPip returns a pip manager. Probe must succeed before use.
func NewPip(log Logger) *Pip { return &Pip{log: log} }

func (p *Pip) ID() string    { return "pip" }
func (p *Pip) Label() string { return "pip" }

func (p *Pip) Kind() string {
	if p.python == "" {
		return "Python packages"
	}
	return "Python packages via " + p.python
}

func (p *Pip) SpecHint() string { return "package  or  package==1.2.3  or  package>=1.0" }

// Probe locates a working interpreter with pip importable, and works out
// whether this host's pip refuses to touch an externally managed environment
// (PEP 668, which is how Debian ships python3-pip).
func (p *Pip) Probe(ctx context.Context) error {
	if p.probed {
		return nil
	}
	candidates := []string{"python3", "python", "python3.13", "python3.12", "python3.11"}
	var lastErr error
	for _, c := range candidates {
		path, ok := lookPath(c)
		if !ok {
			continue
		}
		if _, err := run(ctx, p.log, Command{Name: path, Args: []string{"-m", "pip", "--version"}}); err != nil {
			lastErr = err
			continue
		}
		p.python = path
		p.pipArgs = []string{"-m", "pip"}
		p.detectExternallyManaged(ctx)
		p.probed = true
		return nil
	}
	if lastErr != nil {
		return fmt.Errorf("no usable python with pip: %w", lastErr)
	}
	return errors.New("no python3 interpreter found on PATH")
}

// detectExternallyManaged asks pip to dry-run an install into the environment.
// If it bails out with the PEP 668 error, later installs get the override flag
// so that running as root inside a container behaves the way the user expects.
func (p *Pip) detectExternallyManaged(ctx context.Context) {
	cmd := p.cmd("install", "--dry-run", "--no-deps", "--no-input", "pip")
	_, err := run(ctx, nil, cmd)
	if err == nil {
		return
	}
	var ee *ExecError
	if !errors.As(err, &ee) {
		return
	}
	if !strings.Contains(ee.Stderr, "externally-managed-environment") &&
		!strings.Contains(ee.Stderr, "externally managed") {
		return
	}
	// Confirm the flag exists on this pip before adopting it.
	probe := p.cmd("install", "--dry-run", "--no-deps", "--no-input", "--break-system-packages", "pip")
	if _, err := run(ctx, nil, probe); err == nil {
		p.extraFlags = append(p.extraFlags, "--break-system-packages")
		p.log.log("# pip reports an externally managed environment; using --break-system-packages")
	}
}

// cmd builds a pip invocation with the host specific flags appended.
func (p *Pip) cmd(args ...string) Command {
	full := append([]string{}, p.pipArgs...)
	full = append(full, args...)
	if len(p.extraFlags) > 0 && mutatesEnvironment(args) {
		full = append(full, p.extraFlags...)
	}
	return Command{Name: p.python, Args: full, Env: p.env}
}

func mutatesEnvironment(args []string) bool {
	for _, a := range args {
		switch a {
		case "install", "uninstall", "download":
			return true
		}
	}
	return false
}

// ---------------------------------------------------------------- listing ----

type pipListEntry struct {
	Name              string `json:"name"`
	Version           string `json:"version"`
	LatestVersion     string `json:"latest_version"`
	EditableProjectLo string `json:"editable_project_location"`
}

// List returns everything `pip list` reports as installed.
func (p *Pip) List(ctx context.Context) ([]Package, error) {
	out, err := run(ctx, p.log, p.cmd("list", "--format=json", "--disable-pip-version-check"))
	if err != nil {
		return nil, err
	}
	var entries []pipListEntry
	if err := json.Unmarshal([]byte(jsonSlice(out)), &entries); err != nil {
		return nil, fmt.Errorf("parsing pip list output: %w", err)
	}
	pkgs := make([]Package, 0, len(entries))
	for _, e := range entries {
		pkg := Package{Name: e.Name, Version: e.Version, Installed: true}
		if e.EditableProjectLo != "" {
			pkg.Note = "editable"
		}
		pkgs = append(pkgs, pkg)
	}
	sortPackages(pkgs)
	// Fill in summaries from the metadata pip already has on disk; cheap enough
	// to do in one extra call and it makes the list far more readable.
	if graph, err := p.graph(ctx, names(pkgs)); err == nil {
		for i := range pkgs {
			if n, ok := graph[normalizeName(pkgs[i].Name)]; ok {
				pkgs[i].Summary = n.Summary
			}
		}
	}
	return pkgs, nil
}

// Outdated maps installed package name to the newer version pip found on the
// index. A network failure is reported so the UI can say so rather than
// silently pretending everything is current.
func (p *Pip) Outdated(ctx context.Context) (map[string]string, error) {
	out, err := run(ctx, p.log, p.cmd("list", "--outdated", "--format=json"))
	if err != nil {
		return nil, err
	}
	var entries []pipListEntry
	if err := json.Unmarshal([]byte(jsonSlice(out)), &entries); err != nil {
		return nil, fmt.Errorf("parsing pip list --outdated output: %w", err)
	}
	res := make(map[string]string, len(entries))
	for _, e := range entries {
		if e.LatestVersion != "" {
			res[normalizeName(e.Name)] = e.LatestVersion
		}
	}
	return res, nil
}

// ---------------------------------------------------------------- details ----

// Details prefers `pip show` for installed packages (so Requires and
// Required-by are exactly what pip itself computes) and falls back to the PyPI
// JSON API for packages that are not installed yet.
func (p *Pip) Details(ctx context.Context, pkg Package) (*Details, error) {
	if pkg.Installed {
		if d, err := p.showDetails(ctx, pkg.Name); err == nil {
			return d, nil
		} else if !isNotFound(err) {
			return nil, err
		}
	}
	return p.pypiDetails(ctx, pkg)
}

func (p *Pip) showDetails(ctx context.Context, name string) (*Details, error) {
	out, err := run(ctx, p.log, p.cmd("show", "--verbose", name))
	if err != nil {
		return nil, err
	}
	blocks := parseRFC822(out)
	if len(blocks) == 0 {
		return nil, notFoundError{name}
	}
	b := blocks[0]

	d := &Details{
		Name:      firstNonEmpty(b.get("Name"), name),
		Version:   b.get("Version"),
		Installed: true,
		Source:    p.cmd("show", "--verbose", name).Display(),
		Raw:       strings.TrimSpace(out),
	}
	d.Requires = splitList(b.get("Requires"))
	d.RequiredBy = splitList(b.get("Required-by"))

	add := func(key, value string) {
		if strings.TrimSpace(value) != "" {
			d.Fields = append(d.Fields, Field{Key: key, Value: strings.TrimSpace(value)})
		}
	}
	add("Name", d.Name)
	add("Version", d.Version)
	add("Status", "installed")
	add("Summary", b.get("Summary"))
	add("Home-page", firstNonEmpty(b.get("Home-page"), b.get("Project-URL")))
	add("Author", joinNonEmpty(", ", b.get("Author"), b.get("Author-email")))
	add("Maintainer", joinNonEmpty(", ", b.get("Maintainer"), b.get("Maintainer-email")))
	add("License", oneLine(b.get("License")))
	add("Location", b.get("Location"))
	add("Editable location", b.get("Editable project location"))
	add("Installer", b.get("Installer"))
	add("Requires-Python", b.get("Requires-Python"))
	add("Classifiers", oneLine(b.get("Classifiers")))
	add("Requires", orNone(strings.Join(d.Requires, ", ")))
	add("Required-by", orNone(strings.Join(d.RequiredBy, ", ")))
	add("Files", summarizeFiles(b.get("Files")))
	return d, nil
}

// pypiJSON is the slice of the PyPI JSON API this tool reads.
type pypiJSON struct {
	Info struct {
		Name           string   `json:"name"`
		Version        string   `json:"version"`
		Summary        string   `json:"summary"`
		HomePage       string   `json:"home_page"`
		PackageURL     string   `json:"package_url"`
		Author         string   `json:"author"`
		AuthorEmail    string   `json:"author_email"`
		License        string   `json:"license"`
		RequiresDist   []string `json:"requires_dist"`
		RequiresPython string   `json:"requires_python"`
		Keywords       string   `json:"keywords"`
		Yanked         bool     `json:"yanked"`
	} `json:"info"`
	Releases map[string]json.RawMessage `json:"releases"`
}

func (p *Pip) pypiDetails(ctx context.Context, pkg Package) (*Details, error) {
	url := pypiProjectURL(pkg.Name)
	var doc pypiJSON
	if err := httpJSON(ctx, p.log, url, &doc); err != nil {
		return nil, err
	}
	d := &Details{
		Name:      firstNonEmpty(doc.Info.Name, pkg.Name),
		Version:   doc.Info.Version,
		Installed: false,
		Source:    url,
	}
	// requires_dist carries environment markers; keep only the requirements
	// that apply to a plain install so "direct dependencies" means the same
	// thing as it does for an installed package.
	for _, r := range doc.Info.RequiresDist {
		if name, ok := baseRequirement(r); ok {
			d.Requires = append(d.Requires, name)
		}
	}
	d.Requires = dedupe(d.Requires)

	add := func(key, value string) {
		if strings.TrimSpace(value) != "" {
			d.Fields = append(d.Fields, Field{Key: key, Value: strings.TrimSpace(value)})
		}
	}
	add("Name", d.Name)
	add("Latest version", d.Version)
	status := "not installed"
	if doc.Info.Yanked {
		status += " (latest release is yanked)"
	}
	add("Status", status)
	add("Summary", doc.Info.Summary)
	add("Home-page", firstNonEmpty(doc.Info.HomePage, doc.Info.PackageURL))
	add("Author", joinNonEmpty(", ", doc.Info.Author, doc.Info.AuthorEmail))
	add("License", oneLine(doc.Info.License))
	add("Requires-Python", doc.Info.RequiresPython)
	add("Keywords", doc.Info.Keywords)
	if n := len(doc.Releases); n > 0 {
		add("Releases on PyPI", fmt.Sprintf("%d", n))
		add("Recent versions", strings.Join(recentVersions(doc.Releases, 8), ", "))
	}
	add("Requires", orNone(strings.Join(d.Requires, ", ")))
	add("Index", url)

	var raw strings.Builder
	for _, f := range d.Fields {
		fmt.Fprintf(&raw, "%s: %s\n", f.Key, f.Value)
	}
	d.Raw = raw.String()
	return d, nil
}

// ----------------------------------------------------------------- search ----

// Search returns matching packages. It combines three real sources: the local
// environment, an exact-name lookup on PyPI, and a name search over PyPI's
// Simple index. Each source is best effort; whatever answers contributes
// results, so a search still works offline against what is installed.
func (p *Pip) Search(ctx context.Context, query string) ([]Package, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, errors.New("empty search query")
	}

	installed, err := p.List(ctx)
	if err != nil {
		p.log.log("! could not read installed packages: %v", err)
	}
	installedByName := make(map[string]Package, len(installed))
	for _, pkg := range installed {
		installedByName[normalizeName(pkg.Name)] = pkg
	}

	var (
		results []Package
		seen    = map[string]int{} // normalized name -> index in results
	)
	appendResult := func(pkg Package) {
		key := normalizeName(pkg.Name)
		if key == "" {
			return
		}
		// A package that is installed shows its installed version; anything
		// newer on the index goes in Latest so both are visible at once.
		if local, ok := installedByName[key]; ok {
			if pkg.Version != "" && pkg.Version != local.Version {
				pkg.Latest = pkg.Version
			}
			pkg.Installed = true
			pkg.Version = local.Version
			if pkg.Summary == "" {
				pkg.Summary = local.Summary
			}
		}
		if i, ok := seen[key]; ok {
			// Merge: keep whichever fields we already learnt.
			if results[i].Summary == "" {
				results[i].Summary = pkg.Summary
			}
			if results[i].Latest == "" {
				results[i].Latest = pkg.Latest
			}
			return
		}
		seen[key] = len(results)
		results = append(results, pkg)
	}

	// 1. Installed packages whose name or summary matches.
	q := strings.ToLower(query)
	for _, pkg := range installed {
		if strings.Contains(strings.ToLower(pkg.Name), q) ||
			(pkg.Summary != "" && strings.Contains(strings.ToLower(pkg.Summary), q)) {
			appendResult(pkg)
		}
	}

	// 2. Exact project lookup, including the usual name spellings. This is the
	// fast path: typing a package's real name should not wait on the index.
	var exactErr error
	for _, candidate := range nameVariants(query) {
		var doc pypiJSON
		if err := httpJSON(ctx, nil, pypiProjectURL(candidate), &doc); err != nil {
			exactErr = err
			continue
		}
		appendResult(Package{
			Name:    firstNonEmpty(doc.Info.Name, candidate),
			Version: doc.Info.Version,
			Summary: doc.Info.Summary,
		})
		exactErr = nil
		break
	}

	// 3. Name search across every project on PyPI, for keyword hits.
	hits, searchErr := pypiSearch(ctx, p.log, query)
	if searchErr != nil {
		p.log.log("! the PyPI project index is unavailable: %v", searchErr)
	}
	for _, h := range hits {
		appendResult(h)
	}

	if len(results) == 0 {
		switch {
		case searchErr != nil:
			return nil, fmt.Errorf("nothing matched %q and the PyPI index could not be read: %w",
				query, searchErr)
		case exactErr != nil && !isNotFound(exactErr):
			return nil, fmt.Errorf("nothing matched %q: %w", query, exactErr)
		}
		return nil, fmt.Errorf("no packages match %q", query)
	}
	// Rank exact and prefix matches first, then installed, then the rest.
	sort.SliceStable(results, func(i, j int) bool {
		return searchRank(results[i], q) < searchRank(results[j], q)
	})
	return results, nil
}

func searchRank(p Package, q string) int {
	name := strings.ToLower(p.Name)
	switch {
	case normalizeName(name) == normalizeName(q):
		return 0
	case strings.HasPrefix(name, q):
		return 1
	case p.Installed:
		return 2
	case strings.Contains(name, q):
		return 3
	default:
		return 4
	}
}

// ------------------------------------------------------------------ plans ----

// InstallPlan installs spec verbatim, so version pins such as "flask==3.0.0"
// and extras such as "requests[socks]" work as they do on the command line.
func (p *Pip) InstallPlan(ctx context.Context, spec string) (Plan, error) {
	spec = strings.TrimSpace(spec)
	if spec == "" {
		return Plan{}, errors.New("no package specified")
	}
	if err := validateSpec(spec); err != nil {
		return Plan{}, err
	}
	return Plan{
		Title: "pip install " + spec,
		Steps: []Command{p.cmd("install", "--no-input", spec)},
	}, nil
}

// UpgradePlan moves an installed package to the newest version pip can find.
func (p *Pip) UpgradePlan(ctx context.Context, name string) (Plan, error) {
	name = strings.TrimSpace(name)
	if name == "" {
		return Plan{}, errors.New("no package specified")
	}
	if err := validateSpec(name); err != nil {
		return Plan{}, err
	}
	return Plan{
		Title: "pip install --upgrade " + name,
		Steps: []Command{p.cmd("install", "--no-input", "--upgrade", name)},
	}, nil
}

func (p *Pip) RemovePlan(ctx context.Context, name string) (Plan, error) {
	return p.RemovePlanMany(ctx, []string{name})
}

// RemovePlanMany uninstalls several distributions in one pip invocation.
func (p *Pip) RemovePlanMany(ctx context.Context, names []string) (Plan, error) {
	clean := make([]string, 0, len(names))
	for _, n := range names {
		n = strings.TrimSpace(n)
		if n == "" {
			continue
		}
		if err := validateSpec(n); err != nil {
			return Plan{}, err
		}
		clean = append(clean, n)
	}
	if len(clean) == 0 {
		return Plan{}, errors.New("no package specified")
	}
	args := append([]string{"uninstall", "--yes"}, clean...)
	return Plan{
		Title: "pip uninstall " + strings.Join(clean, " "),
		Steps: []Command{p.cmd(args...)},
	}, nil
}

func (p *Pip) Extras() []Extra {
	return []Extra{
		{Key: "c", Title: "pip check (verify installed dependencies)", Plan: Plan{
			Title: "pip check",
			Steps: []Command{p.cmd("check")},
		}},
		{Key: "C", Title: "pip cache purge (free downloaded wheels)", Plan: Plan{
			Title: "pip cache purge",
			Steps: []Command{p.cmd("cache", "purge")},
		}},
	}
}

// ------------------------------------------------------------- dependency ----

// Snapshot records the installed dependency graph as pip currently sees it.
func (p *Pip) Snapshot(ctx context.Context) (Snapshot, error) {
	pkgs, err := p.List(ctx)
	if err != nil {
		return nil, err
	}
	return p.graph(ctx, names(pkgs))
}

// graph asks pip to describe every named distribution and parses the metadata
// blocks into a name -> requirements map.
func (p *Pip) graph(ctx context.Context, pkgNames []string) (Snapshot, error) {
	snap := Snapshot{}
	if len(pkgNames) == 0 {
		return snap, nil
	}
	const batch = 100 // keep argv comfortably short
	for start := 0; start < len(pkgNames); start += batch {
		end := start + batch
		if end > len(pkgNames) {
			end = len(pkgNames)
		}
		args := append([]string{"show"}, pkgNames[start:end]...)
		out, err := run(ctx, nil, p.cmd(args...))
		if err != nil && strings.TrimSpace(out) == "" {
			return nil, err
		}
		for _, b := range parseRFC822(out) {
			name := b.get("Name")
			if name == "" {
				continue
			}
			snap[normalizeName(name)] = node{
				Version:  b.get("Version"),
				Requires: normalizeAll(splitList(b.get("Requires"))),
				Summary:  oneLine(b.get("Summary")),
			}
		}
	}
	return snap, nil
}

// corePackages are never proposed for removal: pulling them would break the
// environment's ability to manage itself.
var corePackages = map[string]bool{
	"pip": true, "setuptools": true, "wheel": true, "pkg-resources": true,
	"distribute": true, "packaging": true,
}

// PredictOrphans reports which dependencies would become unused if removed were
// uninstalled, without touching anything. The UI shows this in the uninstall
// confirmation so the user knows the full extent of the change up front.
func (p *Pip) PredictOrphans(ctx context.Context, before Snapshot, removed []string) ([]string, error) {
	live := map[string]bool{}
	for name := range before {
		live[name] = true
	}
	for _, r := range normalizeAll(removed) {
		delete(live, r)
	}
	return p.resolveNames(ctx, orphansFrom(before, before, live, removed)), nil
}

// Orphans returns packages that existed only to satisfy the removed set and
// that nothing still installed depends on. It re-reads the live environment, so
// the answer reflects what pip actually did rather than what was predicted.
func (p *Pip) Orphans(ctx context.Context, before Snapshot, removed []string) ([]string, error) {
	after, err := p.Snapshot(ctx)
	if err != nil {
		return nil, err
	}
	live := map[string]bool{}
	for name := range after {
		live[name] = true
	}
	return p.resolveNames(ctx, orphansFrom(before, after, live, removed)), nil
}

// orphansFrom is the dependency-collapse core shared by prediction and the
// post-removal pass.
//
// before is the graph as it looked while removed was still installed: it is the
// only place the removed packages' own requirements are still recorded. current
// and live describe the surviving set.
//
// The question "is this dependency still needed?" is answered by reachability
// rather than by reference counting. A candidate stays only if some surviving
// package that is not itself a candidate requires it, directly or through other
// packages. Counting references would strand a dependency cycle — two packages
// that require each other each look "needed" even when nothing outside the
// cycle wants either — whereas nothing reaches such a cycle from a root, so it
// collapses correctly.
func orphansFrom(before, current Snapshot, live map[string]bool, removed []string) []string {
	removedSet := map[string]bool{}
	for _, r := range normalizeAll(removed) {
		removedSet[r] = true
	}

	// candidates: everything the removed packages pulled in, transitively.
	candidates := map[string]bool{}
	queue := normalizeAll(removed)
	visited := map[string]bool{}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		if visited[cur] {
			continue
		}
		visited[cur] = true
		for _, dep := range before[cur].Requires {
			if !visited[dep] {
				candidates[dep] = true
				queue = append(queue, dep)
			}
		}
	}

	// roots: surviving packages that were not pulled in by the removal. These
	// are what the environment still exists to provide.
	var roots []string
	survivors := map[string]bool{}
	for name := range live {
		if removedSet[name] {
			continue
		}
		survivors[name] = true
		if !candidates[name] {
			roots = append(roots, name)
		}
	}

	// Mark everything the roots depend on, transitively.
	reachable := map[string]bool{}
	queue = roots
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		if reachable[cur] {
			continue
		}
		reachable[cur] = true
		for _, dep := range current[cur].Requires {
			if !reachable[dep] {
				queue = append(queue, dep)
			}
		}
	}

	var orphans []string
	for cand := range candidates {
		if !survivors[cand] || reachable[cand] || corePackages[cand] {
			continue
		}
		orphans = append(orphans, cand)
	}
	sort.Strings(orphans)
	return orphans
}

// resolveNames maps normalized names back to the capitalisation pip reports.
func (p *Pip) resolveNames(ctx context.Context, normalized []string) []string {
	out := make([]string, 0, len(normalized))
	for _, n := range normalized {
		if d, err := p.displayName(ctx, n); err == nil {
			out = append(out, d)
			continue
		}
		out = append(out, n)
	}
	sort.Slice(out, func(i, j int) bool {
		return strings.ToLower(out[i]) < strings.ToLower(out[j])
	})
	return out
}

func (p *Pip) displayName(ctx context.Context, normalized string) (string, error) {
	out, err := run(ctx, nil, p.cmd("show", normalized))
	if err != nil {
		return "", err
	}
	blocks := parseRFC822(out)
	if len(blocks) == 0 || blocks[0].get("Name") == "" {
		return "", notFoundError{normalized}
	}
	return blocks[0].get("Name"), nil
}

// ------------------------------------------------------------------ utils ----

type notFoundError struct{ name string }

func (e notFoundError) Error() string { return e.name + ": not found" }

func isNotFound(err error) bool {
	var nf notFoundError
	if errors.As(err, &nf) {
		return true
	}
	var ee *ExecError
	if errors.As(err, &ee) {
		return strings.Contains(ee.Stderr, "not found") ||
			strings.Contains(ee.Stderr, "WARNING: Package(s) not found")
	}
	return false
}

// specPattern accepts anything that looks like a PEP 508 requirement typed by
// hand, while keeping shell metacharacters out of the argv we build.
var specPattern = regexp.MustCompile(`^[A-Za-z0-9._+\[\]<>=!~,*\- ]+$`)

func validateSpec(spec string) error {
	if !specPattern.MatchString(spec) {
		return fmt.Errorf("%q is not a valid package specifier", spec)
	}
	if strings.HasPrefix(spec, "-") {
		return fmt.Errorf("%q looks like a command-line flag, not a package", spec)
	}
	return nil
}

var normalizeSep = regexp.MustCompile(`[-_.]+`)

// normalizeName applies PEP 503 name normalization.
func normalizeName(name string) string {
	return normalizeSep.ReplaceAllString(strings.ToLower(strings.TrimSpace(name)), "-")
}

func normalizeAll(in []string) []string {
	out := make([]string, 0, len(in))
	for _, s := range in {
		if n := normalizeName(s); n != "" {
			out = append(out, n)
		}
	}
	return out
}

// nameVariants returns the spellings a project might use on PyPI.
func nameVariants(query string) []string {
	base := strings.TrimSpace(query)
	seen := map[string]bool{}
	var out []string
	for _, v := range []string{base, strings.ToLower(base), normalizeName(base),
		strings.ReplaceAll(normalizeName(base), "-", "_")} {
		if v != "" && !seen[v] {
			seen[v] = true
			out = append(out, v)
		}
	}
	return out
}

// baseRequirement extracts the project name from a PEP 508 requirement string,
// skipping requirements that only apply when an extra is requested.
func baseRequirement(req string) (string, bool) {
	if i := strings.IndexByte(req, ';'); i >= 0 {
		marker := req[i+1:]
		if strings.Contains(marker, "extra") {
			return "", false
		}
		req = req[:i]
	}
	req = strings.TrimSpace(req)
	if i := strings.IndexAny(req, " ([<>=!~"); i >= 0 {
		req = req[:i]
	}
	req = strings.TrimSpace(req)
	if req == "" {
		return "", false
	}
	return req, true
}

func names(pkgs []Package) []string {
	out := make([]string, 0, len(pkgs))
	for _, p := range pkgs {
		out = append(out, p.Name)
	}
	return out
}

func dedupe(in []string) []string {
	seen := map[string]bool{}
	out := make([]string, 0, len(in))
	for _, s := range in {
		k := normalizeName(s)
		if k == "" || seen[k] {
			continue
		}
		seen[k] = true
		out = append(out, s)
	}
	return out
}

// jsonSlice trims anything pip printed before the JSON document (warnings on
// stdout are not unheard of).
func jsonSlice(s string) string {
	start := strings.IndexAny(s, "[{")
	if start < 0 {
		return s
	}
	end := strings.LastIndexAny(s, "]}")
	if end < start {
		return s[start:]
	}
	return s[start : end+1]
}

func recentVersions(releases map[string]json.RawMessage, limit int) []string {
	all := make([]string, 0, len(releases))
	for v := range releases {
		all = append(all, v)
	}
	// Newest first, with the raw string as a tiebreak so equal-ranking versions
	// come out in a stable order.
	sort.Slice(all, func(i, j int) bool {
		if c := compareVersions(all[i], all[j]); c != 0 {
			return c > 0
		}
		return all[i] > all[j]
	})
	if len(all) > limit {
		all = all[:limit]
	}
	return all
}

func summarizeFiles(files string) string {
	lines := strings.Split(strings.TrimSpace(files), "\n")
	n := 0
	for _, l := range lines {
		if strings.TrimSpace(l) != "" {
			n++
		}
	}
	if n == 0 {
		return ""
	}
	return fmt.Sprintf("%d installed file(s)", n)
}
