package main

import "testing"

func TestParseDpkgQuery(t *testing.T) {
	// Captured from debian:12-slim, plus a config-files row and a multi-arch
	// row to cover the states that change how a package is classified.
	fixture := "adduser\t3.134\tinstalled\tall\n" +
		"apt\t2.6.1\tinstalled\tarm64\n" +
		"bash\t5.2.15-2+b13\tinstalled\tarm64\n" +
		"gone\t1.0\tconfig-files\tarm64\n" +
		"libfoo:i386\t2.0\tinstalled\ti386\n" +
		"\n"

	pkgs := parseDpkgQuery(fixture)
	if len(pkgs) != 5 {
		t.Fatalf("parsed %d packages, want 5: %+v", len(pkgs), pkgs)
	}

	if got, want := pkgs[0].Name, "adduser"; got != want {
		t.Errorf("name = %q, want %q", got, want)
	}
	if !pkgs[0].FullyInstalled() {
		t.Error("adduser should be fully installed")
	}

	// "config-files" means removed-but-not-purged: known to dpkg, not installed.
	if pkgs[3].FullyInstalled() {
		t.Error("config-files package must not count as installed")
	}
	if got, want := pkgs[3].Status, "config-files"; got != want {
		t.Errorf("status = %q, want %q", got, want)
	}

	// ${binary:Package} appends :arch for foreign architectures; lookups are by
	// bare name, so the suffix must move into Arch.
	if got, want := pkgs[4].Name, "libfoo"; got != want {
		t.Errorf("multi-arch name = %q, want %q", got, want)
	}
	if got, want := pkgs[4].Arch, "i386"; got != want {
		t.Errorf("multi-arch arch = %q, want %q", got, want)
	}
}

func TestParseDpkgQueryMalformed(t *testing.T) {
	// A short row (no tab) carries no version and must be skipped rather than
	// producing a package with an empty version.
	pkgs := parseDpkgQuery("onlyname\n\t\nreal\t1.0\tinstalled\tall\n")
	if len(pkgs) != 1 || pkgs[0].Name != "real" {
		t.Fatalf("malformed rows not skipped: %+v", pkgs)
	}
}

func TestParseDumpavail(t *testing.T) {
	fixture := `Package: nginx
Version: 1.22.1-9
Section: httpd
Depends: libc6, nginx-common
Description: small, powerful, scalable web/proxy server
 Nginx is a web server.

Package: nginx-common
Version: 1.22.1-9
Section: httpd
Description: small, powerful, scalable web/proxy server - common files

Package: nginx
Version: 1.22.1-1
Section: httpd
Description: an older record that must lose
`

	pkgs := parseDumpavail(fixture)
	if len(pkgs) != 2 {
		t.Fatalf("parsed %d packages, want 2 (duplicates collapsed): %+v", len(pkgs), pkgs)
	}

	if got, want := pkgs[0].Name, "nginx"; got != want {
		t.Errorf("name = %q, want %q", got, want)
	}
	// apt emits the preferred version first, so the first record must win.
	if got, want := pkgs[0].Version, "1.22.1-9"; got != want {
		t.Errorf("version = %q, want the first record's %q", got, want)
	}
	if got, want := pkgs[0].Section, "httpd"; got != want {
		t.Errorf("section = %q, want %q", got, want)
	}
	if got, want := pkgs[0].Synopsis, "small, powerful, scalable web/proxy server"; got != want {
		t.Errorf("synopsis = %q, want %q", got, want)
	}
}

func TestParsePolicy(t *testing.T) {
	// Captured from `apt-cache policy adduser libc-bin libssl3`.
	fixture := `adduser:
  Installed: 3.134
  Candidate: 3.134
  Version table:
 *** 3.134 500
        500 http://deb.debian.org/debian bookworm/main arm64 Packages
        100 /var/lib/dpkg/status
libc-bin:
  Installed: 2.36-9+deb12u7
  Candidate: 2.36-9+deb12u14
  Version table:
     2.36-9+deb12u14 500
        500 http://deb.debian.org/debian bookworm/main arm64 Packages
libssl3:
  Installed: (none)
  Candidate: 3.0.20-1~deb12u2
  Version table:
     3.0.20-1~deb12u2 500
`

	entries := parsePolicy(fixture)
	if len(entries) != 3 {
		t.Fatalf("parsed %d entries, want 3: %+v", len(entries), entries)
	}

	if got, want := entries[0].Name, "adduser"; got != want {
		t.Errorf("name = %q, want %q", got, want)
	}
	if entries[0].Installed != entries[0].Candidate {
		t.Errorf("adduser should be up to date, got %+v", entries[0])
	}

	if got, want := entries[1].Installed, "2.36-9+deb12u7"; got != want {
		t.Errorf("installed = %q, want %q", got, want)
	}
	if got, want := entries[1].Candidate, "2.36-9+deb12u14"; got != want {
		t.Errorf("candidate = %q, want %q", got, want)
	}

	// "(none)" must normalise to empty so callers can test for absence.
	if entries[2].Installed != "" {
		t.Errorf("(none) should become empty, got %q", entries[2].Installed)
	}
	if got, want := entries[2].Candidate, "3.0.20-1~deb12u2"; got != want {
		t.Errorf("candidate = %q, want %q", got, want)
	}
}

func TestParsePolicyIgnoresWarnings(t *testing.T) {
	// apt writes "N: Unable to locate package foo" style notes into the stream.
	fixture := "N: Unable to locate package zzz\nadduser:\n  Installed: 3.134\n  Candidate: 3.134\n"
	entries := parsePolicy(fixture)
	if len(entries) != 1 || entries[0].Name != "adduser" {
		t.Fatalf("warning line leaked into results: %+v", entries)
	}
}

func TestParseInstLines(t *testing.T) {
	// Captured from `apt-get -s upgrade` after downgrading libc-bin, plus a
	// fresh-install line of the kind `apt-get -s install` produces.
	fixture := `Reading package lists...
Building dependency tree...
Calculating upgrade...
Inst libc-bin [2.36-9+deb12u7] (2.36-9+deb12u14 Debian:12.15/oldstable [arm64])
Conf libc-bin (2.36-9+deb12u14 Debian:12.15/oldstable [arm64])
Inst brandnew (1.2-3 Debian:12/stable [arm64])
`

	cands := parseInstLines(fixture)
	if len(cands) != 2 {
		t.Fatalf("parsed %d candidates, want 2: %+v", len(cands), cands)
	}

	if got, want := cands[0].Name, "libc-bin"; got != want {
		t.Errorf("name = %q, want %q", got, want)
	}
	if got, want := cands[0].OldVersion, "2.36-9+deb12u7"; got != want {
		t.Errorf("old version = %q, want %q", got, want)
	}
	if got, want := cands[0].NewVersion, "2.36-9+deb12u14"; got != want {
		t.Errorf("new version = %q, want %q", got, want)
	}

	// A fresh install has no bracketed current version.
	if cands[1].OldVersion != "" {
		t.Errorf("fresh install should have no old version, got %q", cands[1].OldVersion)
	}
	if got, want := cands[1].NewVersion, "1.2-3"; got != want {
		t.Errorf("new version = %q, want %q", got, want)
	}
}

func TestParseInstLinesEmptyPlan(t *testing.T) {
	// An up-to-date system produces no Inst lines at all; this is the state that
	// must hold after a successful upgrade.
	fixture := "Reading package lists...\nBuilding dependency tree...\n" +
		"Calculating upgrade...\n0 upgraded, 0 newly installed, 0 to remove.\n"
	if cands := parseInstLines(fixture); len(cands) != 0 {
		t.Errorf("up-to-date plan yielded candidates: %+v", cands)
	}
}

func TestParseRemvLines(t *testing.T) {
	fixture := `Reading package lists...
Building dependency tree...
The following packages will be REMOVED:
  sl libfoo
Remv sl [5.02-1]
Remv libfoo [1.0-2]
`
	names := parseRemvLines(fixture)
	if len(names) != 2 {
		t.Fatalf("parsed %d names, want 2: %v", len(names), names)
	}
	if names[0] != "sl" || names[1] != "libfoo" {
		t.Errorf("names = %v, want [sl libfoo]", names)
	}
}

func TestParseProviders(t *testing.T) {
	// Captured from `apt-cache showpkg awk`, a purely virtual package for which
	// apt-cache show prints nothing at all.
	fixture := `Package: awk
Versions:

Reverse Depends:
  base-files,awk
Dependencies:
Provides:
Reverse Provides:
original-awk 2022-09-12-1 (= )
mawk 1.3.4.20200120-3.1 (= )
gawk 1:5.2.1-2 (= )
`

	providers := parseProviders(fixture)
	want := []string{"original-awk", "mawk", "gawk"}
	if len(providers) != len(want) {
		t.Fatalf("parsed %d providers, want %d: %v", len(providers), len(want), providers)
	}
	for i, w := range want {
		if providers[i] != w {
			t.Errorf("provider %d = %q, want %q", i, providers[i], w)
		}
	}
}

func TestParseProvidersNone(t *testing.T) {
	// A real package has an empty Reverse Provides section.
	fixture := "Package: bash\nVersions: \n5.2.15-2+b13 (...)\n\nReverse Depends: \nDependencies: \nProvides: \nReverse Provides: \n"
	if got := parseProviders(fixture); len(got) != 0 {
		t.Errorf("expected no providers, got %v", got)
	}
}
