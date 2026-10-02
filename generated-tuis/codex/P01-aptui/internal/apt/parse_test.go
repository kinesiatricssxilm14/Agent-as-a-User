package apt

import (
	"reflect"
	"testing"
)

func TestParseUpgradable(t *testing.T) {
	input := `Listing...
curl/bookworm-security 7.88.1-10+deb12u8 amd64 [upgradable from: 7.88.1-10+deb12u7]
libc6:amd64/bookworm 2.36-9+deb12u9 amd64 [upgradable from: 2.36-9+deb12u8]
`
	got := parseUpgradable(input)
	want := map[string]string{
		"curl":  "7.88.1-10+deb12u8",
		"libc6": "2.36-9+deb12u9",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("parseUpgradable() = %#v, want %#v", got, want)
	}
}

func TestParseInstalledIgnoresRemovedPackagesWithConfigFiles(t *testing.T) {
	input := "curl\t7.88.1\tii \nold-package\t1.0\trc \nheld-package\t2.0\thi \n"
	got := parseInstalled(input)
	want := map[string]string{
		"curl":         "7.88.1",
		"held-package": "2.0",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("parseInstalled() = %#v, want %#v", got, want)
	}
}

func TestParseSearch(t *testing.T) {
	input := "curl - command line tool for transferring data with URL syntax\n" +
		"foo - summary containing - another dash\n"
	got := parseSearch(input)
	want := map[string]string{
		"curl": "command line tool for transferring data with URL syntax",
		"foo":  "summary containing - another dash",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("parseSearch() = %#v, want %#v", got, want)
	}
}

func TestParseDetailsAndDependencies(t *testing.T) {
	input := `Package: example
Version: 1.2.3
Architecture: amd64
Maintainer: Example <anonymous@example.invalid>
Depends: libc6 (>= 2.34), foo | bar:any, baz [amd64] <!nocheck>
Pre-Depends: init-system-helpers
Description: short summary
 long description
 .
 another paragraph
Homepage: https://example.test
`
	got, err := parseDetails(input)
	if err != nil {
		t.Fatal(err)
	}
	if got.Name != "example" || got.Version != "1.2.3" {
		t.Fatalf("unexpected details: %#v", got)
	}
	wantDeps := []Dependency{
		{Kind: "Pre-Depends", Name: "init-system-helpers"},
		{Kind: "Depends", Name: "libc6"},
		{Kind: "Depends", Name: "foo"},
		{Kind: "Depends", Name: "bar:any"},
		{Kind: "Depends", Name: "baz"},
	}
	if !reflect.DeepEqual(got.Dependencies, wantDeps) {
		t.Fatalf("dependencies = %#v, want %#v", got.Dependencies, wantDeps)
	}
	if got.Description != "short summary\nlong description\n\nanother paragraph" {
		t.Fatalf("description = %q", got.Description)
	}
}

func TestValidatePackageName(t *testing.T) {
	for _, valid := range []string{"curl", "libc6:amd64", "g++", "foo.bar-1"} {
		if err := validatePackageName(valid); err != nil {
			t.Errorf("%q should be valid: %v", valid, err)
		}
	}
	for _, invalid := range []string{"", "foo bar", "x;id", "../pkg"} {
		if err := validatePackageName(invalid); err == nil {
			t.Errorf("%q should be invalid", invalid)
		}
	}
}
