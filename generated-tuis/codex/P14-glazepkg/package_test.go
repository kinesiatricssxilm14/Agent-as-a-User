package main

import (
	"reflect"
	"testing"
)

func TestParseAptDeps(t *testing.T) {
	got := parseAptDeps("libc6 (>= 2.34), python3:any | python3-minimal, zlib1g")
	want := []string{"libc6", "python3:any", "zlib1g"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("parseAptDeps() = %#v, want %#v", got, want)
	}
}

func TestNormalizePackageName(t *testing.T) {
	if got, want := norm("ImportLib_Metadata"), "importlib-metadata"; got != want {
		t.Fatalf("norm() = %q, want %q", got, want)
	}
}

func TestFilterPackages(t *testing.T) {
	m := newModel()
	m.packages = []pkg{
		{Name: "Requests", Version: "2.32.0"},
		{Name: "urllib3", Version: "2.2.1"},
	}
	m.applyFilter("req")
	if len(m.filtered) != 1 || m.filtered[0].Name != "Requests" {
		t.Fatalf("unexpected filter result: %#v", m.filtered)
	}
}
