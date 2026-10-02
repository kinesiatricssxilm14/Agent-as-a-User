package manager

import (
	"reflect"
	"sort"
	"testing"
)

func TestParsePipShow(t *testing.T) {
	out := `Name: requests
Version: 2.31.0
Summary: Python HTTP for Humans.
Home-page: https://requests.readthedocs.io
Author: Kenneth Reitz
License: Apache 2.0
Location: /usr/lib/python3/dist-packages
Requires: certifi, charset-normalizer, idna, urllib3
Required-by: pip-audit
Description: A simple HTTP library.
  It supports many features.
`
	info := parsePipShow(out)
	if info.Name != "requests" {
		t.Fatalf("Name = %q", info.Name)
	}
	if info.Version != "2.31.0" {
		t.Fatalf("Version = %q", info.Version)
	}
	if info.Summary != "Python HTTP for Humans." {
		t.Fatalf("Summary = %q", info.Summary)
	}
	want := []string{"certifi", "charset-normalizer", "idna", "urllib3"}
	if !reflect.DeepEqual(info.Depends, want) {
		t.Fatalf("Depends = %#v, want %#v", info.Depends, want)
	}
	if !reflect.DeepEqual(info.RequiredBy, []string{"pip-audit"}) {
		t.Fatalf("RequiredBy = %#v", info.RequiredBy)
	}
	if info.Description != "A simple HTTP library.\nIt supports many features." {
		t.Fatalf("Description = %q", info.Description)
	}
}

func TestParseDpkgStatus(t *testing.T) {
	out := `Package: libcurl4
Status: install ok installed
Priority: optional
Section: libs
Installed-Size: 710
Maintainer: Debian
Architecture: amd64
Version: 7.88.1-10+deb12u5
Depends: libbrotli1 (>= 1.0.9), libc6 (>= 2.34), zlib1g (>= 1:1.2.11)
Description: easy-to-use client-side URL transfer library
 libcurl is an easy-to-use client-side URL transfer library.
 .
 This package provides the shared library.
`
	info := parseDpkgStatus(out)
	if info.Name != "libcurl4" {
		t.Fatalf("Name = %q", info.Name)
	}
	if info.Version != "7.88.1-10+deb12u5" {
		t.Fatalf("Version = %q", info.Version)
	}
	if info.Status != "install ok installed" {
		t.Fatalf("Status = %q", info.Status)
	}
	want := []string{"libbrotli1", "libc6", "zlib1g"}
	if !reflect.DeepEqual(info.Depends, want) {
		t.Fatalf("Depends = %#v, want %#v", info.Depends, want)
	}
	if info.Summary != "easy-to-use client-side URL transfer library" {
		t.Fatalf("Summary = %q", info.Summary)
	}
}

func TestAptDepNames(t *testing.T) {
	got := aptDepNames("libc6 (>= 2.34), zlib1g | libz1, python3:any (>= 3.9)")
	want := []string{"libc6", "zlib1g", "python3"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("aptDepNames = %#v, want %#v", got, want)
	}
}

func TestParseDpkgQueryOutput(t *testing.T) {
	out := "installed\tbash\t5.2.15-2\n" +
		"installed\tlibc6\t2.36-9\n" +
		"config-files\toldpkg\t1.0-1\n" +
		"not-installed\tghost\t9.9\n"
	got := parseDpkgQueryOutput(out)
	if len(got) != 2 {
		t.Fatalf("len = %d, got %#v", len(got), got)
	}
	if got[0].Name != "bash" || got[1].Name != "libc6" {
		t.Fatalf("got %#v", got)
	}
}

func TestDependencyClosureAndOrphans(t *testing.T) {
	requires := map[string][]string{
		"a": {"b", "c"},
		"b": {"d"},
		"c": {"e"},
		"d": {},
		"e": {},
		"x": {"c"}, // x (outside the closure) also depends on c
	}

	closure := dependencyClosure(requires, "a")
	if !reflect.DeepEqual(sortedKeys(closure), []string{"b", "c", "d", "e"}) {
		t.Fatalf("closure = %#v", sortedKeys(closure))
	}

	orphans := orphanedDeps(requires, "a", closure)
	// c is protected (x depends on it), and e is protected transitively.
	// b and d have no external reverse deps, so they are orphaned.
	sort.Strings(orphans)
	if !reflect.DeepEqual(orphans, []string{"b", "d"}) {
		t.Fatalf("orphans = %#v, want [b d]", orphans)
	}
}

func sortedKeys(m map[string]bool) []string {
	var out []string
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}
