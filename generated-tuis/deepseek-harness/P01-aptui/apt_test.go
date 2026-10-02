package main

import (
	"reflect"
	"testing"
)

func TestParseInstalled(t *testing.T) {
	out := "acl\t2.3.1-3\tamd64\tii\n" +
		"adduser\t3.134\tall\tii\n" +
		"apt\t2.6.1\tamd64\tii\n" +
		"old-pkg\t1.0\tamd64\trc\n" + // config files only, not installed
		"virtual\t\tall\tii\n" // no version, skip
	got := parseInstalled(out)

	if _, ok := got["old-pkg"]; ok {
		t.Errorf("rc package should not be considered installed")
	}
	if _, ok := got["virtual"]; ok {
		t.Errorf("version-less package should be skipped")
	}
	if len(got) != 3 {
		t.Fatalf("expected 3 installed packages, got %d: %v", len(got), got)
	}
	if got["acl"].InstalledVer != "2.3.1-3" || got["acl"].Status != StatusInstalled {
		t.Errorf("acl parsed wrong: %+v", got["acl"])
	}
}

func TestParseAvailable(t *testing.T) {
	out := "Package: acl\n" +
		"Version: 2.3.1-3\n" +
		"Architecture: amd64\n" +
		"Description: access control list - utilities\n" +
		" This package contains the getfacl and setfacl utilities.\n\n" +
		"Package: zip\n" +
		"Version: 3.0-12\n" +
		"Architecture: amd64\n" +
		"Description: Archiver for .zip files\n"
	got := parseAvailable(out)

	if len(got) != 2 {
		t.Fatalf("expected 2 available packages, got %d", len(got))
	}
	acl := got["acl"]
	if acl.CandidateVer != "2.3.1-3" || acl.Arch != "amd64" || acl.Status != StatusAvailable {
		t.Errorf("acl parsed wrong: %+v", acl)
	}
	if acl.Summary != "access control list - utilities" {
		t.Errorf("summary wrong: %q", acl.Summary)
	}
}

func TestParseUpgradable(t *testing.T) {
	out := "Listing...\n" +
		"openssl/stable-security 3.0.14-1~deb12u1 amd64 [upgradable from: 3.0.13-1~deb12u1]\n" +
		"systemd/stable 252.30-1~deb12u2 amd64 [upgradable from: 252.29-1~deb12u1]\n"
	got := parseUpgradable(out)

	if len(got) != 2 {
		t.Fatalf("expected 2 upgradable packages, got %d", len(got))
	}
	o := got["openssl"]
	if o.Status != StatusUpgradable || o.CandidateVer != "3.0.14-1~deb12u1" || o.InstalledVer != "3.0.13-1~deb12u1" {
		t.Errorf("openssl parsed wrong: %+v", o)
	}
}

func TestParseDeps(t *testing.T) {
	got := parseDeps("libc6 (>= 2.34), libgcc-s1 (>= 3.0), foo | bar, baz")
	want := []string{"libc6 (>= 2.34)", "libgcc-s1 (>= 3.0)", "foo", "bar", "baz"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("parseDeps = %v, want %v", got, want)
	}
	if parseDeps("") != nil {
		t.Errorf("empty field should parse to nil")
	}
}

func TestParseControlBlock(t *testing.T) {
	block := "Package: foo\n" +
		"Version: 1.0\n" +
		"Depends: libc6,\n" +
		" zlib1g\n" +
		"Description: short desc\n" +
		" long line one\n" +
		" long line two\n"
	f := parseControlBlock(block)

	if f["Package"] != "foo" {
		t.Errorf("Package = %q", f["Package"])
	}
	if f["Depends"] != "libc6,\n zlib1g" {
		t.Errorf("Depends = %q", f["Depends"])
	}
	if shortDesc(f["Description"]) != "short desc" {
		t.Errorf("shortDesc = %q", shortDesc(f["Description"]))
	}
	if !reflect.DeepEqual(parseDeps(f["Depends"]), []string{"libc6", "zlib1g"}) {
		t.Errorf("Depends continuation not flattened")
	}
}

func TestMergePackages(t *testing.T) {
	installed := map[string]Package{
		"foo": {Name: "foo", InstalledVer: "1.0", Arch: "amd64", Status: StatusInstalled},
		"bar": {Name: "bar", InstalledVer: "2.0", Arch: "amd64", Status: StatusInstalled},
	}
	available := map[string]Package{
		"foo": {Name: "foo", CandidateVer: "1.2", Arch: "amd64", Status: StatusAvailable, Summary: "foo summary"},
		"baz": {Name: "baz", CandidateVer: "3.0", Arch: "amd64", Status: StatusAvailable, Summary: "baz summary"},
	}
	upgradable := map[string]Package{
		"foo": {Name: "foo", InstalledVer: "1.0", CandidateVer: "1.2", Status: StatusUpgradable},
	}

	all, byName := mergePackages(installed, available, upgradable)

	if len(all) != 3 {
		t.Fatalf("expected 3 merged packages, got %d", len(all))
	}
	if all[0].Name != "bar" || all[1].Name != "baz" || all[2].Name != "foo" {
		t.Errorf("merge not sorted: %v", []string{all[0].Name, all[1].Name, all[2].Name})
	}
	foo := byName["foo"]
	if foo.Status != StatusUpgradable {
		t.Errorf("foo should be upgradable, got %v", foo.Status)
	}
	if foo.Summary != "foo summary" {
		t.Errorf("foo summary missing: %+v", foo)
	}
	if byName["bar"].Status != StatusInstalled {
		t.Errorf("bar should be installed")
	}
	if byName["baz"].Status != StatusAvailable {
		t.Errorf("baz should be available")
	}
}

func TestFilterPackages(t *testing.T) {
	pkgs := []Package{
		{Name: "apache2", Status: StatusInstalled, Summary: "HTTP server"},
		{Name: "zip", Status: StatusAvailable, Summary: "Archiver"},
		{Name: "openssl", Status: StatusUpgradable, Summary: "Secure sockets"},
	}

	if got := filterPackages(pkgs, ModeUpgradable, ""); len(got) != 1 || got[0].Name != "openssl" {
		t.Errorf("upgradable filter wrong: %v", got)
	}
	if got := filterPackages(pkgs, ModeInstalled, ""); len(got) != 2 {
		t.Errorf("installed filter should include upgradable, got %d", len(got))
	}
	if got := filterPackages(pkgs, ModeAvailable, ""); len(got) != 1 || got[0].Name != "zip" {
		t.Errorf("available filter wrong: %v", got)
	}
	if got := filterPackages(pkgs, ModeAll, "SOCK"); len(got) != 1 || got[0].Name != "openssl" {
		t.Errorf("keyword search wrong: %v", got)
	}
	if got := filterPackages(pkgs, ModeAll, "zip"); len(got) != 1 || got[0].Name != "zip" {
		t.Errorf("name search wrong: %v", got)
	}
}
