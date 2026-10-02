package main

import (
	"bufio"
	"strings"
)

// Parsers for the line-oriented output of dpkg-query and apt-get. Like
// control.go these are pure functions so they can be exercised without apt.

// installedPkg is one row of `dpkg-query -W`.
type installedPkg struct {
	Name    string
	Version string
	Status  string // db:Status-Status, e.g. "installed", "config-files"
	Arch    string
}

// FullyInstalled reports whether dpkg considers the package present on the
// system. Packages that were removed but kept their configuration files land in
// state "config-files": dpkg still knows them, but they are not installed and
// must not be counted as such.
func (p installedPkg) FullyInstalled() bool {
	return p.Status == "installed"
}

// parseDpkgQuery parses the tab-separated output of
//
//	dpkg-query -W -f='${binary:Package}\t${Version}\t${db:Status-Status}\t${Architecture}\n'
//
// ${binary:Package} appends ":arch" for packages of a non-native architecture,
// which is stripped so that lookups by plain package name succeed; the
// architecture is preserved in its own field.
func parseDpkgQuery(out string) []installedPkg {
	var pkgs []installedPkg

	scanner := bufio.NewScanner(strings.NewReader(out))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for scanner.Scan() {
		line := strings.TrimRight(scanner.Text(), "\r")
		if strings.TrimSpace(line) == "" {
			continue
		}

		parts := strings.Split(line, "\t")
		if len(parts) < 2 {
			continue
		}

		p := installedPkg{Name: strings.TrimSpace(parts[0])}
		if p.Name == "" {
			continue
		}
		p.Version = strings.TrimSpace(parts[1])
		if len(parts) > 2 {
			p.Status = strings.TrimSpace(parts[2])
		}
		if len(parts) > 3 {
			p.Arch = strings.TrimSpace(parts[3])
		}

		if colon := strings.IndexByte(p.Name, ':'); colon >= 0 {
			if p.Arch == "" {
				p.Arch = p.Name[colon+1:]
			}
			p.Name = p.Name[:colon]
		}

		pkgs = append(pkgs, p)
	}
	return pkgs
}

// availablePkg is the subset of a dumpavail record that the list view needs.
// Long descriptions are deliberately not retained: the available index holds
// ~63k records on Debian 12 and the details pane re-reads the full record from
// apt-cache show on demand.
type availablePkg struct {
	Name     string
	Version  string
	Section  string
	Synopsis string
}

// parseDumpavail parses `apt-cache dumpavail` output. Where several versions of
// the same package are present the first record wins, matching apt's own
// ordering (most preferred first).
func parseDumpavail(out string) []availablePkg {
	var pkgs []availablePkg
	seen := make(map[string]bool)

	for _, rec := range parseControlRecords(out, 0) {
		name := rec.Get("Package")
		if name == "" || seen[name] {
			continue
		}
		seen[name] = true

		synopsis, _ := rec.Description()
		pkgs = append(pkgs, availablePkg{
			Name:     name,
			Version:  rec.Get("Version"),
			Section:  rec.Get("Section"),
			Synopsis: synopsis,
		})
	}
	return pkgs
}

// policyEntry is the installed/candidate pair reported by apt-cache policy.
type policyEntry struct {
	Name      string
	Installed string // "(none)" is normalised to ""
	Candidate string
}

// parsePolicy parses `apt-cache policy <pkg>...` output, which repeats this
// block per package:
//
//	adduser:
//	  Installed: 3.134
//	  Candidate: 3.134
//	  Version table:
//	     ...
//
// The version table is skipped. Note that the header line is unindented while
// every detail line is indented, which is what distinguishes them.
func parsePolicy(out string) []policyEntry {
	var (
		entries []policyEntry
		cur     *policyEntry
	)

	flush := func() {
		if cur != nil && cur.Name != "" {
			entries = append(entries, *cur)
		}
		cur = nil
	}

	scanner := bufio.NewScanner(strings.NewReader(out))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for scanner.Scan() {
		line := strings.TrimRight(scanner.Text(), "\r")
		if strings.TrimSpace(line) == "" {
			continue
		}

		if line[0] != ' ' && line[0] != '\t' {
			// Start of a new package block.
			flush()
			name := strings.TrimSuffix(strings.TrimSpace(line), ":")
			if name == "" || strings.Contains(name, " ") {
				// Not a package header; apt warnings ("N: Unable to locate...")
				// contain spaces and are ignored.
				continue
			}
			if colon := strings.IndexByte(name, ':'); colon >= 0 {
				name = name[:colon]
			}
			cur = &policyEntry{Name: name}
			continue
		}

		if cur == nil {
			continue
		}

		trimmed := strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(trimmed, "Installed:"):
			cur.Installed = normalisePolicyVersion(trimmed[len("Installed:"):])
		case strings.HasPrefix(trimmed, "Candidate:"):
			cur.Candidate = normalisePolicyVersion(trimmed[len("Candidate:"):])
		}
	}

	flush()
	return entries
}

func normalisePolicyVersion(v string) string {
	v = strings.TrimSpace(v)
	if v == "(none)" {
		return ""
	}
	return v
}

// upgradeCandidate is one package that apt would upgrade.
type upgradeCandidate struct {
	Name       string
	OldVersion string
	NewVersion string
}

// parseInstLines extracts the packages apt plans to install or upgrade from the
// output of `apt-get -s upgrade` (or -s install, -s dist-upgrade), whose lines
// look like:
//
//	Inst libc-bin [2.36-9+deb12u7] (2.36-9+deb12u14 Debian:12.15/oldstable [arm64])
//	Inst newpkg (1.2-3 Debian:12/stable [arm64])
//
// The bracketed term before the parenthesis is the currently installed version
// and is absent for a fresh install.
func parseInstLines(out string) []upgradeCandidate {
	var cands []upgradeCandidate

	scanner := bufio.NewScanner(strings.NewReader(out))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if !strings.HasPrefix(line, "Inst ") {
			continue
		}

		rest := strings.TrimSpace(line[len("Inst "):])
		fields := strings.Fields(rest)
		if len(fields) == 0 {
			continue
		}

		c := upgradeCandidate{Name: fields[0]}
		if c.Name == "" {
			continue
		}
		if colon := strings.IndexByte(c.Name, ':'); colon >= 0 {
			c.Name = c.Name[:colon]
		}

		// The parenthesised group holds the new version and itself contains a
		// bracketed architecture, so the old-version bracket is only the one
		// that appears before the parenthesis.
		paren := strings.IndexByte(rest, '(')
		head := rest
		if paren >= 0 {
			head = rest[:paren]
		}
		if open := strings.IndexByte(head, '['); open >= 0 {
			if end := strings.IndexByte(head[open:], ']'); end >= 0 {
				c.OldVersion = strings.TrimSpace(head[open+1 : open+end])
			}
		}
		if paren >= 0 {
			inner := rest[paren+1:]
			if end := strings.IndexByte(inner, ')'); end >= 0 {
				inner = inner[:end]
			}
			if f := strings.Fields(inner); len(f) > 0 {
				c.NewVersion = f[0]
			}
		}

		cands = append(cands, c)
	}
	return cands
}

// parseRemvLines extracts the packages apt plans to remove from the output of
// `apt-get -s remove <pkg>`, whose lines look like:
//
//	Remv sl [5.02-1]
//
// This is how toola tells the user, before they confirm, that removing one
// package will drag others out with it.
func parseRemvLines(out string) []string {
	var names []string

	scanner := bufio.NewScanner(strings.NewReader(out))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if !strings.HasPrefix(line, "Remv ") {
			continue
		}

		fields := strings.Fields(strings.TrimSpace(line[len("Remv "):]))
		if len(fields) == 0 {
			continue
		}
		name := fields[0]
		if colon := strings.IndexByte(name, ':'); colon >= 0 {
			name = name[:colon]
		}
		if name != "" {
			names = append(names, name)
		}
	}
	return names
}

// parseProviders extracts the packages that provide a virtual package from
// `apt-cache showpkg <name>` output. A purely virtual package such as "awk"
// produces no output at all from apt-cache show, so this is the only way to
// give the details pane something real to display.
//
// The relevant section looks like:
//
//	Reverse Provides:
//	original-awk 2022-09-12-1 (= )
//	mawk 1.3.4.20200120-3.1 (= )
func parseProviders(out string) []string {
	var (
		names  []string
		inList bool
		seen   = make(map[string]bool)
	)

	scanner := bufio.NewScanner(strings.NewReader(out))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for scanner.Scan() {
		line := scanner.Text()
		trimmed := strings.TrimSpace(line)

		if strings.HasPrefix(trimmed, "Reverse Provides:") {
			inList = true
			continue
		}
		if !inList {
			continue
		}
		// Any other unindented section header ends the list.
		if trimmed == "" {
			continue
		}
		if strings.HasSuffix(trimmed, ":") && !strings.Contains(trimmed, " ") {
			break
		}

		fields := strings.Fields(trimmed)
		if len(fields) == 0 {
			continue
		}
		name := fields[0]
		if colon := strings.IndexByte(name, ':'); colon >= 0 {
			name = name[:colon]
		}
		if name == "" || seen[name] {
			continue
		}
		seen[name] = true
		names = append(names, name)
	}
	return names
}
