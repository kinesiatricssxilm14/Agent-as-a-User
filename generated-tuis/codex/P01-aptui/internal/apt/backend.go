package apt

import (
	"bufio"
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"sort"
	"strings"
	"time"
)

const commandTimeout = 30 * time.Minute

type Package struct {
	Name             string
	Summary          string
	InstalledVersion string
	CandidateVersion string
	Upgradable       bool
}

func (p Package) Installed() bool {
	return p.InstalledVersion != ""
}

type Dependency struct {
	Kind string
	Name string
}

type Details struct {
	Name             string
	Version          string
	InstalledVersion string
	CandidateVersion string
	Architecture     string
	Section          string
	Maintainer       string
	Homepage         string
	Installed        bool
	Upgradable       bool
	Description      string
	Dependencies     []Dependency
}

type Backend interface {
	Packages(context.Context) ([]Package, error)
	Details(context.Context, string) (Details, error)
	Install(context.Context, string) (string, error)
	Remove(context.Context, string) (string, error)
	Upgrade(context.Context, string) (string, error)
	UpgradeAll(context.Context) (string, error)
	Update(context.Context) (string, error)
}

type SystemBackend struct{}

func NewSystemBackend() *SystemBackend {
	return &SystemBackend{}
}

func (b *SystemBackend) Packages(ctx context.Context) ([]Package, error) {
	ctx, cancel := context.WithTimeout(ctx, commandTimeout)
	defer cancel()

	namesOutput, err := run(ctx, "apt-cache", "pkgnames")
	if err != nil {
		return nil, fmt.Errorf("list available packages: %w", err)
	}
	searchOutput, searchErr := run(ctx, "apt-cache", "search", ".")
	if searchErr != nil {
		// Package names are enough to keep browsing functional if apt's search
		// index is temporarily unavailable.
		searchOutput = ""
	}
	installedOutput, err := run(
		ctx,
		"dpkg-query",
		"-W",
		"-f=${binary:Package}\\t${Version}\\t${db:Status-Abbrev}\\n",
	)
	if err != nil {
		return nil, fmt.Errorf("list installed packages: %w", err)
	}
	upgradableOutput, upgradableErr := run(ctx, "apt", "list", "--upgradable")
	if upgradableErr != nil {
		// apt can emit a non-fatal warning or return a nonzero status when lists are
		// temporarily unavailable. Browsing remains useful without upgrade data.
		upgradableOutput = ""
	}

	installed := parseInstalled(installedOutput)
	upgradable := parseUpgradable(upgradableOutput)
	summaries := parseSearch(searchOutput)
	all := make(map[string]Package, len(installed)+50000)

	scanner := bufio.NewScanner(strings.NewReader(namesOutput))
	for scanner.Scan() {
		name := normalizePackageName(scanner.Text())
		if name == "" {
			continue
		}
		all[name] = Package{Name: name, Summary: summaries[name]}
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("read available package list: %w", err)
	}

	for name, version := range installed {
		pkg := all[name]
		pkg.Name = name
		if pkg.Summary == "" {
			pkg.Summary = summaries[name]
		}
		pkg.InstalledVersion = version
		all[name] = pkg
	}
	for name, candidate := range upgradable {
		pkg := all[name]
		pkg.Name = name
		if pkg.Summary == "" {
			pkg.Summary = summaries[name]
		}
		pkg.Upgradable = true
		pkg.CandidateVersion = candidate
		all[name] = pkg
	}

	packages := make([]Package, 0, len(all))
	for _, pkg := range all {
		packages = append(packages, pkg)
	}
	sort.Slice(packages, func(i, j int) bool {
		return packages[i].Name < packages[j].Name
	})
	return packages, nil
}

func (b *SystemBackend) Details(ctx context.Context, name string) (Details, error) {
	if err := validatePackageName(name); err != nil {
		return Details{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, commandTimeout)
	defer cancel()

	show, err := run(ctx, "apt-cache", "show", "--no-all-versions", name)
	if err != nil {
		return Details{}, fmt.Errorf("read package details: %w", err)
	}
	policy, _ := run(ctx, "apt-cache", "policy", name)
	details, err := parseDetails(show)
	if err != nil {
		return Details{}, err
	}
	details.Name = name
	installed, candidate := parsePolicy(policy)
	details.InstalledVersion = installed
	details.CandidateVersion = candidate
	details.Installed = installed != "" && installed != "(none)"
	details.Upgradable = details.Installed && candidate != "" && candidate != "(none)" && installed != candidate
	if details.Version == "" {
		details.Version = candidate
	}
	return details, nil
}

func (b *SystemBackend) Install(ctx context.Context, name string) (string, error) {
	return b.mutate(ctx, "install", name, "apt-get", "install", "-y", "--", name)
}

func (b *SystemBackend) Remove(ctx context.Context, name string) (string, error) {
	return b.mutate(ctx, "remove", name, "apt-get", "remove", "-y", "--", name)
}

func (b *SystemBackend) Upgrade(ctx context.Context, name string) (string, error) {
	return b.mutate(ctx, "upgrade", name, "apt-get", "install", "--only-upgrade", "-y", "--", name)
}

func (b *SystemBackend) UpgradeAll(ctx context.Context) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, commandTimeout)
	defer cancel()
	return runMutation(ctx, "apt-get", "upgrade", "-y")
}

func (b *SystemBackend) Update(ctx context.Context) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, commandTimeout)
	defer cancel()
	return runMutation(ctx, "apt-get", "update")
}

func (b *SystemBackend) mutate(ctx context.Context, verb, name, command string, args ...string) (string, error) {
	if err := validatePackageName(name); err != nil {
		return "", err
	}
	ctx, cancel := context.WithTimeout(ctx, commandTimeout)
	defer cancel()
	output, err := runMutation(ctx, command, args...)
	if err != nil {
		return output, fmt.Errorf("%s %s: %w", verb, name, err)
	}
	return output, nil
}

func run(ctx context.Context, command string, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, command, args...)
	cmd.Env = append(os.Environ(),
		"LC_ALL=C",
		"LANG=C",
		"DEBIAN_FRONTEND=noninteractive",
	)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	if errors.Is(ctx.Err(), context.DeadlineExceeded) {
		return stdout.String(), fmt.Errorf("command timed out")
	}
	if err != nil {
		message := strings.TrimSpace(stderr.String())
		if message == "" {
			message = strings.TrimSpace(stdout.String())
		}
		if message != "" {
			return stdout.String(), fmt.Errorf("%w: %s", err, lastLines(message, 8))
		}
		return stdout.String(), err
	}
	return stdout.String(), nil
}

func runMutation(ctx context.Context, command string, args ...string) (string, error) {
	output, err := run(ctx, command, args...)
	if err != nil {
		return output, err
	}
	return strings.TrimSpace(output), nil
}

func validatePackageName(name string) error {
	if name == "" {
		return errors.New("package name is empty")
	}
	for _, r := range name {
		if (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') ||
			(r >= '0' && r <= '9') || r == '+' || r == '-' || r == '.' || r == ':' {
			continue
		}
		return fmt.Errorf("invalid package name %q", name)
	}
	return nil
}

func normalizePackageName(name string) string {
	name = strings.TrimSpace(name)
	if i := strings.IndexByte(name, ':'); i >= 0 {
		return name[:i]
	}
	return name
}

func lastLines(value string, count int) string {
	lines := strings.Split(strings.TrimSpace(value), "\n")
	if len(lines) > count {
		lines = lines[len(lines)-count:]
	}
	return strings.Join(lines, "\n")
}
