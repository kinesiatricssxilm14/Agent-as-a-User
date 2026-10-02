package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"html"
	"io"
	"net/http"
	"os/exec"
	"regexp"
	"sort"
	"strings"
	"time"
)

type manager int

const (
	pipManager manager = iota
	aptManager
)

func (m manager) String() string {
	if m == aptManager {
		return "apt"
	}
	return "pip"
}

type pkg struct {
	Name, Version, Summary string
	Installed              bool
}

type detail struct {
	Name, Version, Summary, Homepage, Location string
	Dependencies                               []string
	RequiredBy                                 []string
	Extra                                      string
}

type commandResult struct {
	Output string
	Err    error
}

func run(ctx context.Context, name string, args ...string) commandResult {
	cmd := exec.CommandContext(ctx, name, args...)
	var out bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &out
	err := cmd.Run()
	return commandResult{strings.TrimSpace(out.String()), err}
}

func listPackages(m manager) ([]pkg, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()
	if m == pipManager {
		r := run(ctx, "python3", "-m", "pip", "list", "--format=json", "--disable-pip-version-check")
		if r.Err != nil {
			return nil, fmt.Errorf("pip list: %w: %s", r.Err, r.Output)
		}
		var rows []struct {
			Name    string `json:"name"`
			Version string `json:"version"`
		}
		if err := json.Unmarshal([]byte(r.Output), &rows); err != nil {
			return nil, err
		}
		out := make([]pkg, 0, len(rows))
		for _, p := range rows {
			out = append(out, pkg{Name: p.Name, Version: p.Version, Installed: true})
		}
		sort.Slice(out, func(i, j int) bool { return strings.ToLower(out[i].Name) < strings.ToLower(out[j].Name) })
		return out, nil
	}
	r := run(ctx, "dpkg-query", "-W", "-f=${db:Status-Abbrev}\t${binary:Package}\t${Version}\n")
	if r.Err != nil {
		return nil, fmt.Errorf("apt list: %w: %s", r.Err, r.Output)
	}
	var out []pkg
	s := bufio.NewScanner(strings.NewReader(r.Output))
	for s.Scan() {
		parts := strings.SplitN(s.Text(), "\t", 3)
		if len(parts) == 3 && strings.HasPrefix(parts[0], "ii") {
			out = append(out, pkg{Name: parts[1], Version: parts[2], Installed: true})
		}
	}
	sort.Slice(out, func(i, j int) bool { return strings.ToLower(out[i].Name) < strings.ToLower(out[j].Name) })
	return out, s.Err()
}

func packageDetail(m manager, name string, remote bool) (detail, error) {
	if remote {
		return pypiDetail(name)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if m == pipManager {
		r := run(ctx, "python3", "-m", "pip", "show", name)
		if r.Err != nil {
			return detail{}, fmt.Errorf("pip show: %w: %s", r.Err, r.Output)
		}
		d := detail{Name: name}
		for _, line := range strings.Split(r.Output, "\n") {
			parts := strings.SplitN(line, ":", 2)
			if len(parts) != 2 {
				continue
			}
			v := strings.TrimSpace(parts[1])
			switch strings.ToLower(parts[0]) {
			case "name":
				d.Name = v
			case "version":
				d.Version = v
			case "summary":
				d.Summary = v
			case "home-page":
				d.Homepage = v
			case "location":
				d.Location = v
			case "requires":
				d.Dependencies = splitCSV(v)
			case "required-by":
				d.RequiredBy = splitCSV(v)
			}
		}
		return d, nil
	}
	r := run(ctx, "apt-cache", "show", name)
	if r.Err != nil {
		return detail{}, fmt.Errorf("apt-cache show: %w: %s", r.Err, r.Output)
	}
	d := detail{Name: name}
	for _, line := range strings.Split(r.Output, "\n") {
		if line == "" && d.Version != "" {
			break
		}
		parts := strings.SplitN(line, ":", 2)
		if len(parts) != 2 {
			continue
		}
		v := strings.TrimSpace(parts[1])
		switch parts[0] {
		case "Package":
			d.Name = v
		case "Version":
			d.Version = v
		case "Description":
			d.Summary = v
		case "Homepage":
			d.Homepage = v
		case "Depends":
			d.Dependencies = parseAptDeps(v)
		}
	}
	r2 := run(ctx, "apt-cache", "rdepends", "--installed", name)
	if r2.Err == nil {
		for _, v := range strings.Split(r2.Output, "\n")[1:] {
			v = strings.TrimSpace(v)
			if v != "" && !strings.HasPrefix(v, "Reverse Depends") {
				d.RequiredBy = append(d.RequiredBy, v)
			}
		}
	}
	return d, nil
}

func splitCSV(v string) []string {
	if strings.TrimSpace(v) == "" {
		return nil
	}
	xs := strings.Split(v, ",")
	for i := range xs {
		xs[i] = strings.TrimSpace(xs[i])
	}
	return xs
}
func parseAptDeps(v string) []string {
	var out []string
	for _, x := range strings.Split(v, ",") {
		x = strings.TrimSpace(strings.Split(x, "|")[0])
		if i := strings.Index(x, " ("); i >= 0 {
			x = x[:i]
		}
		if x != "" {
			out = append(out, x)
		}
	}
	return out
}

func pipArgs(args ...string) []string {
	base := []string{"-m", "pip"}
	base = append(base, args...)
	base = append(base, "--disable-pip-version-check", "--break-system-packages")
	return base
}

func mutatePackage(m manager, action, name string) commandResult {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Minute)
	defer cancel()
	if m == pipManager {
		switch action {
		case "install":
			return run(ctx, "python3", pipArgs("install", name)...)
		case "upgrade":
			return run(ctx, "python3", pipArgs("install", "--upgrade", name)...)
		case "uninstall":
			return uninstallPipWithOrphans(ctx, name)
		}
	}
	switch action {
	case "install":
		return run(ctx, "apt-get", "install", "-y", name)
	case "upgrade":
		return run(ctx, "apt-get", "install", "-y", "--only-upgrade", name)
	case "uninstall":
		return run(ctx, "apt-get", "remove", "-y", name)
	}
	return commandResult{Err: errors.New("unknown package operation")}
}

type inspectReport struct {
	Installed []struct {
		Metadata struct {
			Name     string   `json:"name"`
			Requires []string `json:"requires_dist"`
		} `json:"metadata"`
	} `json:"installed"`
}

var reqNameRE = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]*`)

func norm(s string) string {
	return strings.ToLower(strings.ReplaceAll(strings.ReplaceAll(s, "_", "-"), ".", "-"))
}
func pipGraph(ctx context.Context) (map[string]string, map[string][]string) {
	r := run(ctx, "python3", "-m", "pip", "inspect", "--local", "--disable-pip-version-check")
	names := map[string]string{}
	graph := map[string][]string{}
	if r.Err != nil {
		return names, graph
	}
	var rep inspectReport
	if json.Unmarshal([]byte(r.Output), &rep) != nil {
		return names, graph
	}
	for _, p := range rep.Installed {
		n := norm(p.Metadata.Name)
		names[n] = p.Metadata.Name
		for _, raw := range p.Metadata.Requires {
			if strings.Contains(raw, "extra ==") || strings.Contains(raw, "extra !=") {
				continue
			}
			dep := reqNameRE.FindString(strings.TrimSpace(raw))
			if dep != "" {
				graph[n] = append(graph[n], norm(dep))
			}
		}
	}
	return names, graph
}
func uninstallPipWithOrphans(ctx context.Context, name string) commandResult {
	names, graph := pipGraph(ctx)
	target := norm(name)
	closure := map[string]bool{}
	var walk func(string)
	walk = func(n string) {
		for _, d := range graph[n] {
			if names[d] != "" && !closure[d] {
				closure[d] = true
				walk(d)
			}
		}
	}
	walk(target)
	r := run(ctx, "python3", pipArgs("uninstall", "-y", name)...)
	if r.Err != nil {
		return r
	}
	removed := []string{}
	protected := map[string]bool{"pip": true, "setuptools": true, "wheel": true}
	for {
		changed := false
		for candidate := range closure {
			if protected[candidate] || names[candidate] == "" {
				continue
			}
			needed := false
			for parent, deps := range graph {
				if parent == target || names[parent] == "" {
					continue
				}
				for _, d := range deps {
					if d == candidate {
						needed = true
						break
					}
				}
				if needed {
					break
				}
			}
			if !needed {
				actual := names[candidate]
				rr := run(ctx, "python3", pipArgs("uninstall", "-y", actual)...)
				if rr.Err == nil {
					removed = append(removed, actual)
					delete(names, candidate)
					changed = true
				}
			}
		}
		if !changed {
			break
		}
	}
	if len(removed) > 0 {
		r.Output += "\nAuto-removed unused dependencies: " + strings.Join(removed, ", ")
	}
	return r
}

var snippetRE = regexp.MustCompile(`(?s)<a[^>]+class="package-snippet"[^>]+href="/project/([^/]+)/"[^>]*>.*?<span class="package-snippet__version">([^<]*)</span>.*?<p class="package-snippet__description">(.*?)</p>`)
var tagsRE = regexp.MustCompile(`<[^>]+>`)

func searchPyPI(q string) ([]pkg, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, "GET", "https://pypi.org/search/?q="+urlQueryEscape(q), nil)
	req.Header.Set("User-Agent", "tooln/1.0 package TUI")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	b, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("PyPI returned %s", resp.Status)
	}
	matches := snippetRE.FindAllStringSubmatch(string(b), -1)
	out := make([]pkg, 0, len(matches))
	for _, m := range matches {
		summary := strings.TrimSpace(html.UnescapeString(tagsRE.ReplaceAllString(m[3], " ")))
		out = append(out, pkg{Name: html.UnescapeString(m[1]), Version: strings.TrimSpace(html.UnescapeString(m[2])), Summary: summary})
	}
	if len(out) == 0 {
		d, e := pypiDetail(q)
		if e == nil {
			out = append(out, pkg{Name: d.Name, Version: d.Version, Summary: d.Summary})
		}
	}
	if len(out) == 0 {
		return nil, errors.New("no PyPI packages found (try an exact package name)")
	}
	return out, nil
}
func urlQueryEscape(s string) string { // sufficient and dependency-free for query values
	r := strings.NewReplacer("%", "%25", " ", "+", "+", "%2B", "&", "%26", "?", "%3F", "#", "%23")
	return r.Replace(s)
}
func pypiDetail(name string) (detail, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, "GET", "https://pypi.org/pypi/"+urlQueryEscape(name)+"/json", nil)
	req.Header.Set("User-Agent", "tooln/1.0 package TUI")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return detail{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != 200 {
		return detail{}, fmt.Errorf("PyPI returned %s", resp.Status)
	}
	var x struct {
		Info struct {
			Name         string   `json:"name"`
			Version      string   `json:"version"`
			Summary      string   `json:"summary"`
			HomePage     string   `json:"home_page"`
			PackageURL   string   `json:"package_url"`
			RequiresDist []string `json:"requires_dist"`
		} `json:"info"`
	}
	if err = json.NewDecoder(resp.Body).Decode(&x); err != nil {
		return detail{}, err
	}
	deps := []string{}
	for _, raw := range x.Info.RequiresDist {
		if n := reqNameRE.FindString(raw); n != "" {
			deps = append(deps, n)
		}
	}
	return detail{Name: x.Info.Name, Version: x.Info.Version, Summary: x.Info.Summary, Homepage: firstNonempty(x.Info.HomePage, x.Info.PackageURL), Dependencies: deps, Extra: "PyPI search result — press i to install"}, nil
}
func firstNonempty(a, b string) string {
	if a != "" {
		return a
	}
	return b
}
