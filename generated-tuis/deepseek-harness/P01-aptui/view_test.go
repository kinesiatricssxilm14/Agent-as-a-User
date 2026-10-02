package main

import (
	"strings"
	"testing"
)

func samplePackages() []Package {
	return []Package{
		{Name: "apache2", InstalledVer: "2.4.57-2", CandidateVer: "2.4.57-2", Arch: "amd64", Status: StatusInstalled, Summary: "Apache HTTP Server"},
		{Name: "openssl", InstalledVer: "3.0.13", CandidateVer: "3.0.14", Arch: "amd64", Status: StatusUpgradable, Summary: "Secure Sockets Layer toolkit"},
		{Name: "zip", InstalledVer: "", CandidateVer: "3.0-12", Arch: "amd64", Status: StatusAvailable, Summary: "Archiver for .zip files"},
	}
}

func TestViewRendersWithoutPanic(t *testing.T) {
	m := initialModel()
	m.width, m.height, m.ready = 120, 32, true

	pkgs := samplePackages()
	m.allPackages = pkgs
	m.pkgByName = map[string]Package{}
	for _, p := range pkgs {
		m.pkgByName[p.Name] = p
	}
	m.availableMap = map[string]Package{"zip": pkgs[2]}
	m.installedMap = map[string]Package{"apache2": pkgs[0], "openssl": pkgs[1]}
	m.upgradable = map[string]Package{"openssl": pkgs[1]}
	m.loading = false
	m.statusMsg = "loaded 3 packages"

	m.rebuildList()

	v := m.View()
	if v == "" {
		t.Fatal("View returned empty string")
	}
	for _, want := range []string{"toola", "apache2", "openssl", "zip", "install", "remove", "upgrade"} {
		if !strings.Contains(v, want) {
			t.Errorf("View missing %q", want)
		}
	}
}

func TestViewRunningStateHasOutput(t *testing.T) {
	m := initialModel()
	m.width, m.height, m.ready = 100, 30, true
	m.state = StateRunning
	m.statusMsg = "running install zip…"
	m.lastActionDesc = "install zip"
	m.output = []string{"Get:1 http://deb.debian.org stable InRelease", "Setting up zip (3.0-12) ..."}
	m.outputView.SetContent(strings.Join(m.output, "\n"))
	m.outputView.Width = 100
	m.outputView.Height = 8

	v := m.View()
	if !strings.Contains(v, "Setting up zip") {
		t.Errorf("running state should show command output, got: %q", v)
	}
}

func TestRenderDetailsListsDepsOnePerLine(t *testing.T) {
	d := PackageDetails{
		Name:         "libfoo",
		Status:       StatusUpgradable,
		InstalledVer: "1.0",
		CandidateVer: "1.2",
		Arch:         "amd64",
		Description:  "full description line one\nline two",
		Depends:      []string{"libc6 (>= 2.34)", "zlib1g"},
		Recommends:   []string{"libbar"},
	}
	out := renderDetails(d)

	if !strings.Contains(out, "libc6 (>= 2.34)") || !strings.Contains(out, "zlib1g") {
		t.Errorf("dependencies missing from details: %q", out)
	}
	if !strings.Contains(out, "line one") || !strings.Contains(out, "line two") {
		t.Errorf("full description missing: %q", out)
	}
	if !strings.Contains(out, "installed (upgradable)") {
		t.Errorf("upgradable status missing: %q", out)
	}
	// Each dependency must be on its own line, not collapsed into a summary.
	lines := strings.Split(out, "\n")
	for _, want := range []string{"libc6 (>= 2.34)", "zlib1g", "libbar"} {
		found := false
		for _, ln := range lines {
			if strings.Contains(ln, want) {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("dependency %q not listed on its own line", want)
		}
	}
}
