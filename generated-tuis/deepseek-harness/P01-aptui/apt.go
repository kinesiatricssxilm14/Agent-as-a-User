package main

import (
	"bytes"
	"os"
	"os/exec"
	"sort"
	"strings"
)

// aptEnv returns the environment used for every apt/dpkg invocation so that
// package operations never block on interactive prompts and produce stable,
// machine-parseable (English) output.
func aptEnv() []string {
	env := os.Environ()
	return append(env,
		"DEBIAN_FRONTEND=noninteractive",
		"LC_ALL=C.UTF-8",
		"APT_LISTCHANGES_FRONTEND=none",
		"APT_LISTBUGS_FRONTEND=none",
	)
}

// runCapture runs a command and returns its combined stdout+stderr.
func runCapture(name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Env = aptEnv()
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out
	err := cmd.Run()
	return out.String(), err
}

// dpkgQueryFormat lists package name, installed version, architecture and the
// abbreviated dpkg status word. The trailing status character 'i' means the
// package is actually installed (as opposed to "config-files remaining" etc.).
const dpkgQueryFormat = "-f=${Package}\\t${Version}\\t${Architecture}\\t${db:Status-Abbrev}\\n"

// loadInstalled returns all installed packages keyed by name.
func loadInstalled() (map[string]Package, error) {
	out, err := runCapture("dpkg-query", "-W", dpkgQueryFormat)
	res := parseInstalled(out)
	if err != nil && len(res) == 0 {
		return res, err
	}
	return res, nil
}

// parseInstalled parses `dpkg-query -W` output into installed packages.
func parseInstalled(out string) map[string]Package {
	res := map[string]Package{}
	for _, line := range strings.Split(out, "\n") {
		line = strings.TrimRight(line, "\r")
		fields := strings.Split(line, "\t")
		if len(fields) < 4 {
			continue
		}
		status := fields[3]
		if len(status) < 2 || status[len(status)-1] != 'i' {
			continue
		}
		if fields[1] == "" {
			continue
		}
		res[fields[0]] = Package{
			Name:         fields[0],
			InstalledVer: fields[1],
			Arch:         fields[2],
			Status:       StatusInstalled,
		}
	}
	return res
}

// loadAvailable returns all packages known to apt (the repository index) keyed
// by name, with candidate version, architecture and short description.
func loadAvailable() (map[string]Package, error) {
	out, err := runCapture("apt-cache", "dumpavail")
	res := parseAvailable(out)
	if err != nil && len(res) == 0 {
		return res, err
	}
	return res, nil
}

// parseAvailable parses `apt-cache dumpavail` output (dpkg status format with
// blank-line-separated stanzas).
func parseAvailable(out string) map[string]Package {
	res := map[string]Package{}
	for _, block := range splitStanzas(out) {
		f := parseControlBlock(block)
		name := f["Package"]
		ver := f["Version"]
		if name == "" || ver == "" {
			continue
		}
		res[name] = Package{
			Name:         name,
			CandidateVer: ver,
			Arch:         f["Architecture"],
			Status:       StatusAvailable,
			Summary:      shortDesc(f["Description"]),
		}
	}
	return res
}

// loadUpgradable returns packages that have an available upgrade, keyed by name.
func loadUpgradable() (map[string]Package, error) {
	out, err := runCapture("apt", "list", "--upgradable")
	res := parseUpgradable(out)
	if err != nil && len(res) == 0 {
		return res, err
	}
	return res, nil
}

// parseUpgradable parses `apt list --upgradable` output.
func parseUpgradable(out string) map[string]Package {
	res := map[string]Package{}
	for _, line := range strings.Split(out, "\n") {
		line = strings.TrimSpace(line)
		if !strings.Contains(line, "[upgradable") {
			continue
		}
		slash := strings.IndexByte(line, '/')
		if slash <= 0 {
			continue
		}
		name := line[:slash]
		// apt list format is: name/archive version arch [upgradable from: X]
		// so the token after the slash is the archive/suite, not the version.
		rest := strings.Fields(line[slash+1:])
		candidate := ""
		if len(rest) >= 2 {
			candidate = rest[1]
		}
		installed := ""
		if i := strings.Index(line, "from:"); i >= 0 {
			installed = strings.TrimSuffix(strings.TrimSpace(line[i+len("from:"):]), "]")
		}
		res[name] = Package{
			Name:         name,
			InstalledVer: installed,
			CandidateVer: candidate,
			Status:       StatusUpgradable,
		}
	}
	return res
}

// mergePackages combines installed, available and upgradable maps into a single
// sorted slice plus a by-name lookup. Status precedence is upgradable >
// installed > available, matching reality (an upgradable package is installed).
func mergePackages(installed, available, upgradable map[string]Package) ([]Package, map[string]Package) {
	names := map[string]struct{}{}
	for n := range installed {
		names[n] = struct{}{}
	}
	for n := range available {
		names[n] = struct{}{}
	}
	for n := range upgradable {
		names[n] = struct{}{}
	}

	all := make([]Package, 0, len(names))
	byName := make(map[string]Package, len(names))
	for n := range names {
		p := Package{Name: n}
		if av, ok := available[n]; ok {
			p.CandidateVer = av.CandidateVer
			p.Arch = av.Arch
			p.Summary = av.Summary
			p.Status = StatusAvailable
		}
		if inst, ok := installed[n]; ok {
			p.InstalledVer = inst.InstalledVer
			if p.Arch == "" {
				p.Arch = inst.Arch
			}
			p.Status = StatusInstalled
		}
		if up, ok := upgradable[n]; ok {
			p.Status = StatusUpgradable
			p.CandidateVer = up.CandidateVer
			if up.InstalledVer != "" {
				p.InstalledVer = up.InstalledVer
			}
		}
		all = append(all, p)
		byName[n] = p
	}
	sort.Slice(all, func(i, j int) bool { return all[i].Name < all[j].Name })
	return all, byName
}

// fetchDetails builds the full detail record for a package by querying
// apt-cache (candidate metadata) and, when apt has no record for it, falling
// back to dpkg-query (installed metadata).
func fetchDetails(p Package) PackageDetails {
	d := PackageDetails{
		Name:         p.Name,
		Status:       p.Status,
		InstalledVer: p.InstalledVer,
		CandidateVer: p.CandidateVer,
		Arch:         p.Arch,
		Summary:      p.Summary,
	}

	if out, err := runCapture("apt-cache", "show", p.Name); err == nil && strings.TrimSpace(out) != "" {
		f := parseControlBlock(splitStanzas(out)[0])
		d.CandidateVer = f["Version"]
		if f["Architecture"] != "" {
			d.Arch = f["Architecture"]
		}
		d.Maintainer = f["Maintainer"]
		d.Homepage = f["Homepage"]
		d.InstalledSize = f["Installed-Size"]
		d.Summary = shortDesc(f["Description"])
		d.Description = strings.TrimSpace(f["Description"])
		d.Depends = parseDeps(f["Depends"])
		d.PreDepends = parseDeps(f["Pre-Depends"])
		d.Recommends = parseDeps(f["Recommends"])
		d.Suggests = parseDeps(f["Suggests"])
		d.Conflicts = parseDeps(f["Conflicts"])
		d.Breaks = parseDeps(f["Breaks"])
		d.Replaces = parseDeps(f["Replaces"])
		d.Provides = parseDeps(f["Provides"])
	}

	if d.Description == "" {
		if out, err := runCapture("dpkg-query", "-s", p.Name); err == nil {
			f := parseControlBlock(out)
			d.Description = strings.TrimSpace(f["Description"])
			if d.Summary == "" {
				d.Summary = shortDesc(f["Description"])
			}
			if d.Maintainer == "" {
				d.Maintainer = f["Maintainer"]
			}
			if d.InstalledVer == "" {
				d.InstalledVer = f["Version"]
			}
		}
	}
	return d
}

// splitStanzas splits dpkg status format on blank lines.
func splitStanzas(data string) []string {
	return strings.Split(data, "\n\n")
}

// parseControlBlock parses a single RFC822-style control stanza. Continuation
// lines (starting with space/tab) are appended to the previous field.
func parseControlBlock(block string) map[string]string {
	res := map[string]string{}
	cur := ""
	for _, line := range strings.Split(block, "\n") {
		line = strings.TrimRight(line, "\r")
		if line == "" {
			continue
		}
		if (line[0] == ' ' || line[0] == '\t') && cur != "" {
			res[cur] += "\n" + line
			continue
		}
		idx := strings.IndexByte(line, ':')
		if idx < 0 {
			continue
		}
		key := strings.TrimSpace(line[:idx])
		val := strings.TrimLeft(line[idx+1:], " \t")
		cur = key
		res[key] = val
	}
	return res
}

// shortDesc returns the first line of a Description field (the short summary).
func shortDesc(desc string) string {
	if i := strings.IndexByte(desc, '\n'); i >= 0 {
		desc = desc[:i]
	}
	return strings.TrimSpace(desc)
}

// parseDeps splits a dependency field into one entry per line. Comma-separated
// items and pipe-separated alternatives each become their own line so the
// details panel lists every dependency package name individually.
func parseDeps(field string) []string {
	field = strings.ReplaceAll(field, "\n", " ")
	field = strings.TrimSpace(field)
	if field == "" {
		return nil
	}
	var out []string
	for _, part := range strings.Split(field, ",") {
		for _, alt := range strings.Split(part, "|") {
			t := strings.TrimSpace(alt)
			if t != "" {
				out = append(out, t)
			}
		}
	}
	return out
}
