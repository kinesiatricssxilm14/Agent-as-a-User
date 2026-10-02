package manager

import (
	"context"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"sync"

	"github.com/tooln/tooln/internal/pkgs"
	"github.com/tooln/tooln/internal/runner"
)

// Apt manages Debian system packages through dpkg-query / apt-cache / apt-get.
type Apt struct {
	updateOnce sync.Once
	updateErr  error
}

// NewApt returns an apt backend.
func NewApt() *Apt { return &Apt{} }

// Name implements pkgs.Manager.
func (a *Apt) Name() string { return "apt" }

// ensureUpdated runs "apt-get update" once so search/install see current
// package lists. Failures are recorded but do not abort individual operations.
func (a *Apt) ensureUpdated(ctx context.Context) error {
	a.updateOnce.Do(func() {
		out, err := runner.RunString(ctx, "apt-get", "update", "-qq")
		if err != nil {
			a.updateErr = fmt.Errorf("apt-get update: %w\n%s", err, runner.Trim(out))
		}
	})
	return a.updateErr
}

// List implements pkgs.Manager using dpkg-query, keeping only packages whose
// status is "installed" (so removed packages with leftover config files are
// excluded).
func (a *Apt) List(ctx context.Context) ([]pkgs.Package, error) {
	out, err := runner.RunString(ctx, "dpkg-query", "-W", "-f=${db:Status-Status}\t${binary:Package}\t${Version}\n")
	if err != nil {
		return nil, fmt.Errorf("dpkg-query failed: %w\n%s", err, runner.Trim(out))
	}
	return parseDpkgQueryOutput(out), nil
}

// parseDpkgQueryOutput parses the "status\tname\tversion" lines produced by
// dpkg-query -W and keeps only installed packages.
func parseDpkgQueryOutput(out string) []pkgs.Package {
	var result []pkgs.Package
	for _, line := range strings.Split(out, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "\t", 3)
		if len(parts) < 3 {
			continue
		}
		status := strings.TrimSpace(parts[0])
		name := strings.TrimSpace(parts[1])
		version := strings.TrimSpace(parts[2])
		if status != "installed" || name == "" {
			continue
		}
		result = append(result, pkgs.Package{Name: name, Version: version, Manager: "apt"})
	}
	sort.Slice(result, func(i, j int) bool {
		return strings.ToLower(result[i].Name) < strings.ToLower(result[j].Name)
	})
	return result
}

// Info implements pkgs.Manager using dpkg -s.
func (a *Apt) Info(ctx context.Context, name string) (*pkgs.PackageInfo, error) {
	out, err := runner.RunString(ctx, "dpkg", "-s", name)
	if err != nil {
		return nil, fmt.Errorf("dpkg -s failed: %s", runner.Trim(out))
	}
	return parseDpkgStatus(out), nil
}

// parseDpkgStatus parses the "Field: value" output of `dpkg -s`.
func parseDpkgStatus(out string) *pkgs.PackageInfo {
	info := &pkgs.PackageInfo{}
	var descLines []string
	inDescription := false
	for _, line := range strings.Split(out, "\n") {
		if strings.HasPrefix(line, " ") || strings.HasPrefix(line, "\t") {
			if inDescription {
				descLines = append(descLines, strings.TrimSpace(line))
			}
			continue
		}
		inDescription = false
		i := strings.Index(line, ":")
		if i <= 0 {
			continue
		}
		key := strings.TrimSpace(line[:i])
		val := strings.TrimSpace(line[i+1:])
		switch key {
		case "Package":
			info.Name = val
		case "Version":
			info.Version = val
		case "Status":
			info.Status = val
		case "Installed-Size":
			info.InstalledSize = val + " kB"
		case "Depends", "Pre-Depends":
			info.Depends = append(info.Depends, aptDepNames(val)...)
		case "Description":
			info.Summary = val
			descLines = []string{}
			inDescription = true
		}
	}
	info.Description = strings.Join(descLines, "\n")
	return info
}

// aptDepNames extracts bare package names from an apt dependency list such as
// "libc6 (>= 2.34), zlib1g | libz1". Architecture qualifiers and alternatives
// are collapsed to the first name of each alternative group.
func aptDepNames(dep string) []string {
	var names []string
	for _, group := range strings.Split(dep, ",") {
		group = strings.TrimSpace(group)
		if group == "" {
			continue
		}
		first := strings.SplitN(group, "|", 2)[0]
		first = strings.TrimSpace(first)
		if first == "" {
			continue
		}
		name := strings.Fields(first)[0]
		if idx := strings.Index(name, ":"); idx > 0 {
			name = name[:idx] // strip :arch qualifier
		}
		if name != "" {
			names = append(names, name)
		}
	}
	return names
}

// Search implements pkgs.Manager using apt-cache search. The query is escaped
// so it is matched as a literal substring rather than a regular expression.
func (a *Apt) Search(ctx context.Context, query string) ([]pkgs.SearchResult, error) {
	_ = a.ensureUpdated(ctx)
	pattern := regexp.QuoteMeta(strings.TrimSpace(query))
	if pattern == "" {
		return nil, nil
	}
	out, err := runner.RunString(ctx, "apt-cache", "search", pattern)
	if err != nil {
		return nil, fmt.Errorf("apt-cache search failed: %w\n%s", err, runner.Trim(out))
	}
	var results []pkgs.SearchResult
	for _, line := range strings.Split(out, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		name, desc := "", ""
		if i := strings.Index(line, " - "); i > 0 {
			name = strings.TrimSpace(line[:i])
			desc = strings.TrimSpace(line[i+3:])
		} else {
			name = strings.Fields(line)[0]
		}
		if name == "" {
			continue
		}
		results = append(results, pkgs.SearchResult{Name: name, Summary: desc})
		if len(results) >= 200 {
			break
		}
	}
	return results, nil
}

// Install implements pkgs.Manager.
func (a *Apt) Install(ctx context.Context, name string) (pkgs.OpResult, error) {
	_ = a.ensureUpdated(ctx)
	out, err := runner.RunString(ctx, "apt-get", "install", "-y", name)
	return pkgs.OpResult{Output: out}, err
}

// Uninstall implements pkgs.Manager.
func (a *Apt) Uninstall(ctx context.Context, name string) (pkgs.OpResult, error) {
	out, err := runner.RunString(ctx, "apt-get", "remove", "-y", name)
	return pkgs.OpResult{Output: out}, err
}

// Upgrade implements pkgs.Manager.
func (a *Apt) Upgrade(ctx context.Context, name string) (pkgs.OpResult, error) {
	_ = a.ensureUpdated(ctx)
	out, err := runner.RunString(ctx, "apt-get", "install", "--only-upgrade", "-y", name)
	return pkgs.OpResult{Output: out}, err
}

// SupportsUpgrade implements pkgs.Manager.
func (a *Apt) SupportsUpgrade() bool { return true }
