package main

import (
	"strings"
	"testing"
)

// testStore builds a store from compact literal fixtures covering every state
// combination the UI has to render.
func testStore() *store {
	s := newStore()

	s.setInstalled([]installedPkg{
		{Name: "bash", Version: "5.2.15-2+b13", Status: "installed", Arch: "arm64"},
		{Name: "libc-bin", Version: "2.36-9+deb12u7", Status: "installed", Arch: "arm64"},
		{Name: "localonly", Version: "0.1", Status: "installed", Arch: "arm64"},
		{Name: "removedpkg", Version: "1.0", Status: "config-files", Arch: "arm64"},
	})
	s.setAvailable([]availablePkg{
		{Name: "bash", Version: "5.2.15-2+b13", Section: "shells", Synopsis: "GNU Bourne Again SHell"},
		{Name: "libc-bin", Version: "2.36-9+deb12u14", Section: "libs", Synopsis: "GNU C Library: Binaries"},
		{Name: "nginx", Version: "1.22.1-9", Section: "httpd", Synopsis: "small, powerful web server"},
		{Name: "removedpkg", Version: "1.0", Section: "misc", Synopsis: "a removed package"},
	})
	s.setUpgradable([]upgradeCandidate{
		{Name: "libc-bin", OldVersion: "2.36-9+deb12u7", NewVersion: "2.36-9+deb12u14"},
		// A brand-new dependency pulled in by the upgrade: not user-upgradable.
		{Name: "brandnew", OldVersion: "", NewVersion: "1.0"},
	})
	s.rebuild()
	return s
}

func TestStoreMergeAndCounts(t *testing.T) {
	s := testStore()

	if got, want := s.countInstalled, 3; got != want {
		t.Errorf("installed count = %d, want %d (config-files excluded)", got, want)
	}
	if got, want := s.countUpgradable, 1; got != want {
		t.Errorf("upgradable count = %d, want %d (fresh installs excluded)", got, want)
	}
	if got, want := s.countResidual, 1; got != want {
		t.Errorf("residual count = %d, want %d", got, want)
	}

	bash := s.lookup("bash")
	if !bash.Installed || bash.InstalledVersion != "5.2.15-2+b13" {
		t.Errorf("bash merge wrong: %+v", bash)
	}
	if bash.Section != "shells" || bash.Synopsis == "" {
		t.Errorf("bash did not pick up metadata from the available index: %+v", bash)
	}
	if bash.Upgradable {
		t.Error("bash should not be upgradable")
	}

	libc := s.lookup("libc-bin")
	if !libc.Upgradable || libc.UpgradeTarget != "2.36-9+deb12u14" {
		t.Errorf("libc-bin upgrade state wrong: %+v", libc)
	}

	// A package installed from a .deb and absent from every repo must survive.
	local := s.lookup("localonly")
	if local == nil || !local.Installed || local.Available {
		t.Errorf("locally installed package mishandled: %+v", local)
	}

	// A fresh dependency named only in the upgrade plan is known but neither
	// installed nor upgradable.
	if bn := s.lookup("brandnew"); bn == nil || bn.Installed || bn.Upgradable {
		t.Errorf("brandnew state wrong: %+v", bn)
	}

	removed := s.lookup("removedpkg")
	if removed.Installed || !removed.ResidualConfig() {
		t.Errorf("removedpkg should be residual-config only: %+v", removed)
	}
}

// names returns the visible package names, for concise assertions.
func names(s *store) []string {
	var out []string
	for _, p := range s.visible() {
		out = append(out, p.Name)
	}
	return out
}

func TestStoreFilters(t *testing.T) {
	s := testStore()

	tests := []struct {
		mode filterMode
		want []string
	}{
		{filterAll, []string{"bash", "brandnew", "libc-bin", "localonly", "nginx", "removedpkg"}},
		{filterInstalled, []string{"bash", "libc-bin", "localonly"}},
		{filterAvailable, []string{"nginx", "removedpkg"}},
		{filterUpgradable, []string{"libc-bin"}},
		{filterResidual, []string{"removedpkg"}},
	}

	for _, tt := range tests {
		t.Run(tt.mode.String(), func(t *testing.T) {
			s.filter = tt.mode
			s.query = ""
			s.rebuild()

			got := names(s)
			if strings.Join(got, ",") != strings.Join(tt.want, ",") {
				t.Errorf("filter %v = %v, want %v", tt.mode, got, tt.want)
			}
		})
	}
}

func TestStoreUpgradeRemovesFromUpgradableList(t *testing.T) {
	// This is the spec's post-operation consistency requirement: after a real
	// upgrade, a rescan yields no Inst line for the package, so it must vanish
	// from the upgradable filter.
	s := testStore()
	s.filter = filterUpgradable
	s.rebuild()

	if got := names(s); len(got) != 1 || got[0] != "libc-bin" {
		t.Fatalf("precondition failed, upgradable = %v", got)
	}

	// Simulate what the post-operation refresh does: new dpkg state, new plan.
	s.setInstalled([]installedPkg{
		{Name: "bash", Version: "5.2.15-2+b13", Status: "installed"},
		{Name: "libc-bin", Version: "2.36-9+deb12u14", Status: "installed"},
		{Name: "localonly", Version: "0.1", Status: "installed"},
	})
	s.setUpgradable(nil)
	s.rebuild()

	if got := names(s); len(got) != 0 {
		t.Errorf("after upgrade the upgradable list should be empty, got %v", got)
	}
	if s.countUpgradable != 0 {
		t.Errorf("upgradable count = %d, want 0", s.countUpgradable)
	}
	if v := s.lookup("libc-bin").InstalledVersion; v != "2.36-9+deb12u14" {
		t.Errorf("installed version not refreshed: %q", v)
	}
}

func TestStoreRemoveClearsInstalledState(t *testing.T) {
	s := testStore()

	// After `apt-get purge localonly`, dpkg stops reporting it entirely.
	s.setInstalled([]installedPkg{
		{Name: "bash", Version: "5.2.15-2+b13", Status: "installed"},
		{Name: "libc-bin", Version: "2.36-9+deb12u7", Status: "installed"},
	})
	s.rebuild()

	p := s.lookup("localonly")
	if p == nil {
		t.Fatal("entry should be retained, not deleted")
	}
	if p.Installed || p.InstalledVersion != "" || p.Status != "" {
		t.Errorf("purged package still looks installed: %+v", p)
	}

	s.filter = filterInstalled
	s.rebuild()
	for _, n := range names(s) {
		if n == "localonly" {
			t.Error("purged package still in the installed filter")
		}
	}
}

func TestStoreSearchRanking(t *testing.T) {
	s := newStore()
	s.setAvailable([]availablePkg{
		{Name: "nginx-extras", Synopsis: "nginx with extra modules"},
		{Name: "libnginx-mod-http-geoip", Synopsis: "GeoIP module"},
		{Name: "nginx", Synopsis: "web server"},
		{Name: "nginx-common", Synopsis: "common files"},
		{Name: "apache2", Synopsis: "Apache HTTP Server, an nginx alternative"},
	})

	s.query = "nginx"
	s.rebuild()

	got := names(s)
	// Exact match first, then prefix matches alphabetically, then substring
	// matches, then synopsis-only matches.
	want := []string{"nginx", "nginx-common", "nginx-extras", "libnginx-mod-http-geoip", "apache2"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("ranking = %v, want %v", got, want)
	}
}

func TestStoreSearchIsCaseInsensitiveAndTrims(t *testing.T) {
	s := newStore()
	s.setAvailable([]availablePkg{
		{Name: "NGINX-weird", Synopsis: "x"},
		{Name: "other", Synopsis: "y"},
	})

	s.query = "  NgInX  "
	s.rebuild()

	if got := names(s); len(got) != 1 || got[0] != "NGINX-weird" {
		t.Errorf("case-insensitive trimmed search = %v", got)
	}
}

func TestStoreSearchNoMatches(t *testing.T) {
	s := testStore()
	s.query = "definitely-not-a-package-zzz"
	s.rebuild()

	if got := names(s); len(got) != 0 {
		t.Errorf("expected no matches, got %v", got)
	}
	// Counts describe the whole store and must not be affected by the query.
	if s.countInstalled == 0 {
		t.Error("counts should be independent of the search query")
	}
}

func TestStoreEmptyQueryStaysAlphabetical(t *testing.T) {
	s := testStore()
	s.query = ""
	s.rebuild()

	got := names(s)
	for i := 1; i < len(got); i++ {
		if got[i-1] > got[i] {
			t.Fatalf("not sorted at %d: %v", i, got)
		}
	}
}

func TestStoreApplyPolicyDoesNotClobber(t *testing.T) {
	s := testStore()

	s.applyPolicy([]policyEntry{
		// dumpavail already knew a version for nginx; policy must not override.
		{Name: "nginx", Installed: "", Candidate: "9.9.9"},
		// localonly had no candidate; policy fills it in.
		{Name: "localonly", Installed: "0.1", Candidate: "0.2"},
	})
	s.rebuild()

	if got, want := s.lookup("nginx").AvailableVersion, "1.22.1-9"; got != want {
		t.Errorf("nginx version = %q, want the dumpavail value %q", got, want)
	}
	local := s.lookup("localonly")
	if !local.Available || local.AvailableVersion != "0.2" {
		t.Errorf("policy did not fill in the missing candidate: %+v", local)
	}
}

func TestStoreInstalledNames(t *testing.T) {
	s := testStore()
	got := s.installedNames()
	want := []string{"bash", "libc-bin", "localonly"}

	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("installedNames = %v, want %v", got, want)
	}
}

func TestStoreDisplayVersion(t *testing.T) {
	s := testStore()

	if got, want := s.lookup("bash").DisplayVersion(), "5.2.15-2+b13"; got != want {
		t.Errorf("installed package shows %q, want %q", got, want)
	}
	if got, want := s.lookup("nginx").DisplayVersion(), "1.22.1-9"; got != want {
		t.Errorf("uninstalled package shows %q, want the candidate %q", got, want)
	}
}
