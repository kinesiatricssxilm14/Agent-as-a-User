package main

import (
	"strings"
	"testing"
)

// The fixtures below are verbatim output captured from a real debian:12-slim
// container, so the parsers are tested against the exact bytes apt emits.

const bashShowFixture = `Package: bash
Essential: yes
Status: install ok installed
Priority: required
Section: shells
Installed-Size: 7295
Maintainer: Matthias Klose <anonymous@example.invalid>
Architecture: arm64
Multi-Arch: foreign
Source: bash (5.2.15-2)
Version: 5.2.15-2+b13
Replaces: bash-completion (<< 20060301-0), bash-doc (<= 2.05-1)
Depends: base-files (>= 2.1.12), debianutils (>= 5.6-0.1)
Pre-Depends: libc6 (>= 2.36), libtinfo6 (>= 6)
Recommends: bash-completion (>= 20060301-0)
Suggests: bash-doc
Conflicts: bash-completion (<< 20060301-0)
Conffiles:
 /etc/bash.bashrc 89269e1298235f1b12b4c16e4065ad0d
 /etc/skel/.bash_logout 22bfb8c1dd94b5f3813a2b25da67463f
Description: GNU Bourne Again SHell
 Bash is an sh-compatible command language interpreter that executes
 commands read from the standard input or from a file.
 .
 Bash also incorporates useful features from the Korn and C shells.
Homepage: http://tiswww.case.edu/php/chet/bash/bashtop.html
`

func TestParseControlBash(t *testing.T) {
	rec := parseControl(bashShowFixture)

	if got, want := rec.Get("Package"), "bash"; got != want {
		t.Errorf("Package = %q, want %q", got, want)
	}
	if got, want := rec.Get("Version"), "5.2.15-2+b13"; got != want {
		t.Errorf("Version = %q, want %q", got, want)
	}
	// Field lookup must be case-insensitive: sources disagree on capitalisation.
	if got, want := rec.Get("vErSiOn"), "5.2.15-2+b13"; got != want {
		t.Errorf("case-insensitive Get = %q, want %q", got, want)
	}
	if got, want := rec.Get("Installed-Size"), "7295"; got != want {
		t.Errorf("Installed-Size = %q, want %q", got, want)
	}
	if rec.Get("nonexistent") != "" {
		t.Errorf("missing field should be empty, got %q", rec.Get("nonexistent"))
	}
}

func TestDescriptionSynopsisAndLong(t *testing.T) {
	rec := parseControl(bashShowFixture)
	synopsis, long := rec.Description()

	if got, want := synopsis, "GNU Bourne Again SHell"; got != want {
		t.Errorf("synopsis = %q, want %q", got, want)
	}
	if !strings.HasPrefix(long, "Bash is an sh-compatible") {
		t.Errorf("long description should start with the first body line, got %q", long)
	}
	// The lone "." marker must become a real blank line, not a literal dot.
	if strings.Contains(long, "\n.\n") {
		t.Errorf("long description still contains a literal %q marker: %q", ".", long)
	}
	if !strings.Contains(long, "\n\n") {
		t.Errorf("long description should contain a blank line, got %q", long)
	}
	if !strings.Contains(long, "Korn and C shells") {
		t.Errorf("long description truncated: %q", long)
	}
}

func TestDescriptionEmpty(t *testing.T) {
	rec := parseControl("Package: x\nVersion: 1\n")
	synopsis, long := rec.Description()
	if synopsis != "" || long != "" {
		t.Errorf("absent Description should yield empty strings, got (%q, %q)", synopsis, long)
	}
}

func TestParseControlTakesFirstRecordOnly(t *testing.T) {
	// apt-cache show libssl3 really does emit two records, newest first.
	two := "Package: libssl3\nVersion: 3.0.20-1~deb12u2\n\nPackage: libssl3\nVersion: 3.0.17-1~deb12u2\n"

	rec := parseControl(two)
	if got, want := rec.Get("Version"), "3.0.20-1~deb12u2"; got != want {
		t.Errorf("Version = %q, want the first (candidate) version %q", got, want)
	}

	all := parseControlRecords(two, 0)
	if len(all) != 2 {
		t.Fatalf("parseControlRecords(limit 0) returned %d records, want 2", len(all))
	}
	if got, want := all[1].Get("Version"), "3.0.17-1~deb12u2"; got != want {
		t.Errorf("second record Version = %q, want %q", got, want)
	}
}

func TestParseControlIgnoresAptErrorText(t *testing.T) {
	// `apt-cache show zzznope` writes this to stderr; if it is merged into the
	// stream it must not become a phantom record.
	rec := parseControl("E: No packages found\n")
	if !rec.Empty() {
		t.Errorf("apt error text produced fields: %v", rec.fields)
	}
}

func TestParseDepends(t *testing.T) {
	tests := []struct {
		name  string
		field string
		want  []string // dependency.String() of each entry
	}{
		{
			name:  "empty",
			field: "",
			want:  nil,
		},
		{
			name:  "simple versioned pair",
			field: "base-files (>= 2.1.12), debianutils (>= 5.6-0.1)",
			want:  []string{"base-files (>= 2.1.12)", "debianutils (>= 5.6-0.1)"},
		},
		{
			name:  "unconstrained",
			field: "libc6, zlib1g",
			want:  []string{"libc6", "zlib1g"},
		},
		{
			name:  "alternatives",
			field: "debconf (>= 0.5) | debconf-2.0",
			want:  []string{"debconf (>= 0.5) | debconf-2.0"},
		},
		{
			name:  "three alternatives",
			field: "awk | mawk | gawk",
			want:  []string{"awk | mawk | gawk"},
		},
		{
			name:  "arch qualifier",
			field: "python3:any, libfoo:amd64 (>= 1.0)",
			want:  []string{"python3:any", "libfoo:amd64 (>= 1.0)"},
		},
		{
			name:  "folded across lines",
			field: "libc6 (>= 2.36),\n libssl3 (>= 3.0.0)",
			want:  []string{"libc6 (>= 2.36)", "libssl3 (>= 3.0.0)"},
		},
		{
			name:  "trailing comma and blank entries",
			field: "libc6, , ",
			want:  []string{"libc6"},
		},
		{
			name:  "arch restriction list is not part of the name",
			field: "gcc [amd64 arm64], make <!nocheck>",
			want:  []string{"gcc", "make"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			deps := parseDepends(tt.field)
			if len(deps) != len(tt.want) {
				t.Fatalf("parseDepends(%q) returned %d entries, want %d: %v",
					tt.field, len(deps), len(tt.want), deps)
			}
			for i, want := range tt.want {
				if got := deps[i].String(); got != want {
					t.Errorf("entry %d = %q, want %q", i, got, want)
				}
			}
		})
	}
}

func TestParseDependsNameExtraction(t *testing.T) {
	// The details pane lists bare package names, so Name must be clean.
	deps := parseDepends("libc6 (>= 2.36), python3:any, debconf | debconf-2.0")
	want := []string{"libc6", "python3", "debconf"}

	if len(deps) != len(want) {
		t.Fatalf("got %d deps, want %d", len(deps), len(want))
	}
	for i, w := range want {
		if deps[i].Name != w {
			t.Errorf("dep %d Name = %q, want %q", i, deps[i].Name, w)
		}
	}
	if got, want := deps[1].Arch, "any"; got != want {
		t.Errorf("arch qualifier = %q, want %q", got, want)
	}
	if len(deps[2].Alternatives) != 1 || deps[2].Alternatives[0].Name != "debconf-2.0" {
		t.Errorf("alternative not captured: %+v", deps[2].Alternatives)
	}
}

func TestRelationsOrderAndCoverage(t *testing.T) {
	groups := parseControl(bashShowFixture).relations()

	// Pre-Depends must lead, and every non-empty relation must appear: the spec
	// requires all dependency names to be listed, not just Depends.
	if len(groups) == 0 {
		t.Fatal("no relation groups parsed")
	}
	if groups[0].Field != "Pre-Depends" {
		t.Errorf("first group = %q, want Pre-Depends", groups[0].Field)
	}

	seen := map[string]int{}
	for _, g := range groups {
		seen[g.Field] = len(g.Deps)
	}
	for field, wantCount := range map[string]int{
		"Pre-Depends": 2,
		"Depends":     2,
		"Recommends":  1,
		"Suggests":    1,
		"Conflicts":   1,
		"Replaces":    2,
	} {
		if seen[field] != wantCount {
			t.Errorf("group %q has %d deps, want %d", field, seen[field], wantCount)
		}
	}
	// Fields absent from the record must not produce empty groups.
	if _, ok := seen["Provides"]; ok {
		t.Error("absent Provides field produced a group")
	}
}
