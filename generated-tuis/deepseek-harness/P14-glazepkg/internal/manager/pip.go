// Package manager implements the pip and apt package-manager backends.
package manager

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strings"

	"github.com/tooln/tooln/internal/pkgs"
	"github.com/tooln/tooln/internal/runner"
)

// Pip manages Python packages through the "python3 -m pip" entry point.
type Pip struct{}

// NewPip returns a pip backend.
func NewPip() *Pip { return &Pip{} }

// Name implements pkgs.Manager.
func (p *Pip) Name() string { return "pip" }

// pipArgs prefixes the arguments used for every pip invocation.
func pipArgs(args ...string) []string {
	return append([]string{"-m", "pip"}, args...)
}

// pipListEntry is one element of "pip list --format=json" output.
type pipListEntry struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

// List implements pkgs.Manager by running "pip list --format=json".
func (p *Pip) List(ctx context.Context) ([]pkgs.Package, error) {
	out, err := runner.RunString(ctx, "python3", pipArgs("list", "--format=json")...)
	if err != nil {
		return nil, fmt.Errorf("pip list failed: %w\n%s", err, runner.Trim(out))
	}
	var entries []pipListEntry
	if err := json.Unmarshal([]byte(out), &entries); err != nil {
		return nil, fmt.Errorf("parsing pip list output: %w", err)
	}
	pkgs2 := make([]pkgs.Package, 0, len(entries))
	for _, e := range entries {
		if e.Name == "" {
			continue
		}
		pkgs2 = append(pkgs2, pkgs.Package{Name: e.Name, Version: e.Version, Manager: "pip"})
	}
	sort.Slice(pkgs2, func(i, j int) bool {
		return strings.ToLower(pkgs2[i].Name) < strings.ToLower(pkgs2[j].Name)
	})
	return pkgs2, nil
}

// Info implements pkgs.Manager using "pip show <name>".
func (p *Pip) Info(ctx context.Context, name string) (*pkgs.PackageInfo, error) {
	out, err := runner.RunString(ctx, "python3", pipArgs("show", name)...)
	if err != nil {
		// pip show exits non-zero for an unknown package.
		return nil, fmt.Errorf("pip show failed: %s", runner.Trim(out))
	}
	return parsePipShow(out), nil
}

// parsePipShow parses the "Key: value" output of `pip show`.
func parsePipShow(out string) *pkgs.PackageInfo {
	info := &pkgs.PackageInfo{}
	var descLines []string
	key := ""
	for _, line := range strings.Split(out, "\n") {
		if strings.HasPrefix(line, " ") || strings.HasPrefix(line, "\t") {
			// Continuation of a multi-line value.
			if key == "Description" {
				descLines = append(descLines, strings.TrimSpace(line))
			}
			continue
		}
		if i := strings.Index(line, ":"); i > 0 {
			key = strings.TrimSpace(line[:i])
			val := strings.TrimSpace(line[i+1:])
			switch key {
			case "Name":
				info.Name = val
			case "Version":
				info.Version = val
			case "Summary":
				info.Summary = val
			case "Home-page":
				info.HomePage = val
			case "Author":
				info.Author = val
			case "License":
				info.License = val
			case "Location":
				info.Location = val
			case "Requires":
				info.Depends = splitNames(val)
			case "Required-by":
				info.RequiredBy = splitNames(val)
			case "Description":
				descLines = []string{val}
			}
		}
	}
	info.Description = strings.Join(descLines, "\n")
	return info
}

// splitNames splits a comma/space separated list of package names into a
// clean slice, dropping empty entries.
func splitNames(s string) []string {
	if s == "" {
		return nil
	}
	fields := strings.FieldsFunc(s, func(r rune) bool {
		return r == ',' || r == ' '
	})
	out := make([]string, 0, len(fields))
	for _, f := range fields {
		f = strings.TrimSpace(f)
		if f != "" {
			out = append(out, f)
		}
	}
	return out
}

// Search implements pkgs.Manager by querying the Python Package Index.
func (p *Pip) Search(ctx context.Context, query string) ([]pkgs.SearchResult, error) {
	return SearchPypi(ctx, query)
}

// Install implements pkgs.Manager. It transparently retries with
// --break-system-packages when the target environment is PEP 668 managed.
func (p *Pip) Install(ctx context.Context, name string) (pkgs.OpResult, error) {
	return pipMutate(ctx, "install", name)
}

// Upgrade implements pkgs.Manager.
func (p *Pip) Upgrade(ctx context.Context, name string) (pkgs.OpResult, error) {
	return pipMutate(ctx, "install", "--upgrade", name)
}

// SupportsUpgrade implements pkgs.Manager.
func (p *Pip) SupportsUpgrade() bool { return true }

// pipMutate runs a pip install/uninstall command, retrying once with
// --break-system-packages if the environment rejects the first attempt.
func pipMutate(ctx context.Context, args ...string) (pkgs.OpResult, error) {
	out, err := runner.RunString(ctx, "python3", pipArgs(args...)...)
	if err == nil {
		return pkgs.OpResult{Output: out}, nil
	}
	if strings.Contains(out, "externally-managed-environment") {
		// PEP 668 managed environment: retry with the escape hatch.
		args2 := append([]string{args[0], "--break-system-packages"}, args[1:]...)
		out2, err2 := runner.RunString(ctx, "python3", pipArgs(args2...)...)
		return pkgs.OpResult{Output: out2}, err2
	}
	return pkgs.OpResult{Output: out}, err
}

// Uninstall implements pkgs.Manager. It removes the target package with pip
// and then removes any now-orphaned direct/transitive dependencies (packages
// that are no longer required by any remaining installed package).
func (p *Pip) Uninstall(ctx context.Context, name string) (pkgs.OpResult, error) {
	requires, err := pipRequiresMap(ctx)
	if err != nil {
		// If we cannot compute the dependency graph, still perform the basic
		// uninstall so the requested package is removed.
		res, uerr := pipMutate(ctx, "uninstall", "-y", name)
		return res, uerr
	}

	target := strings.ToLower(name)
	closure := dependencyClosure(requires, target)
	orphans := orphanedDeps(requires, target, closure)

	res, err := pipMutate(ctx, "uninstall", "-y", name)
	if err != nil {
		return res, err
	}
	var outputs []string
	if s := strings.TrimSpace(res.Output); s != "" {
		outputs = append(outputs, s)
	}

	if len(orphans) > 0 {
		sort.Strings(orphans)
		args := append([]string{"uninstall", "-y"}, orphans...)
		oRes, oErr := pipMutate(ctx, args...)
		if s := strings.TrimSpace(oRes.Output); s != "" {
			outputs = append(outputs, "autoremove: "+s)
		}
		if oErr != nil {
			return pkgs.OpResult{Output: strings.Join(outputs, "\n"), Removed: orphans}, oErr
		}
	}
	return pkgs.OpResult{Output: strings.Join(outputs, "\n"), Removed: orphans}, nil
}

// pipRequiresMap builds name -> direct dependency names (lower-cased) for every
// installed distribution using importlib.metadata. Using the Python standard
// library mirrors how pip itself reads package metadata.
func pipRequiresMap(ctx context.Context) (map[string][]string, error) {
	const script = `
import importlib.metadata as md, json, sys
out = {}
for dist in md.distributions():
    try:
        name = dist.metadata.get("Name")
    except Exception:
        continue
    if not name:
        continue
    reqs = []
    for r in (dist.requires or []):
        try:
            reqs.append(r.name.lower())
        except Exception:
            pass
    out[name.lower()] = sorted(set(reqs))
json.dump(out, sys.stdout)
`
	out, err := runner.RunString(ctx, "python3", "-c", script)
	if err != nil {
		return nil, fmt.Errorf("reading distribution metadata: %w", err)
	}
	var m map[string][]string
	if err := json.Unmarshal([]byte(out), &m); err != nil {
		return nil, fmt.Errorf("parsing distribution metadata: %w", err)
	}
	return m, nil
}

// dependencyClosure returns the transitive set of dependency names reachable
// from start (excluding start itself) within the requires map.
func dependencyClosure(requires map[string][]string, start string) map[string]bool {
	seen := map[string]bool{}
	stack := []string{start}
	for len(stack) > 0 {
		cur := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		for _, dep := range requires[cur] {
			if dep == start || seen[dep] {
				continue
			}
			seen[dep] = true
			stack = append(stack, dep)
		}
	}
	return seen
}

// orphanedDeps determines which members of the target's dependency closure can
// be safely removed: those not required (directly or transitively) by any
// installed package that lives outside the closure.
func orphanedDeps(requires map[string][]string, target string, closure map[string]bool) []string {
	// Seeds: closure packages required by some package outside the closure.
	keep := map[string]bool{}
	for pkg, deps := range requires {
		if pkg == target {
			continue
		}
		if closure[pkg] {
			continue // dependency itself is inside the closure
		}
		for _, d := range deps {
			if closure[d] {
				keep[d] = true
			}
		}
	}
	// Propagate: anything a kept package requires must itself be kept.
	stack := make([]string, 0, len(keep))
	for k := range keep {
		stack = append(stack, k)
	}
	for len(stack) > 0 {
		cur := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		for _, d := range requires[cur] {
			if closure[d] && !keep[d] {
				keep[d] = true
				stack = append(stack, d)
			}
		}
	}
	var orphans []string
	for pkg := range closure {
		if !keep[pkg] {
			orphans = append(orphans, pkg)
		}
	}
	return orphans
}
