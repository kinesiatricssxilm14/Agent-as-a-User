package conflict

import (
	"strings"
	"testing"
)

func TestParseSingleConflict(t *testing.T) {
	src := "line before\n<<<<<<< HEAD\nmain branch text\n=======\nfeature branch text\n>>>>>>> feature\nline after\n"
	f, err := Parse("conflict.py", src)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if got := f.ConflictCount(); got != 1 {
		t.Fatalf("conflict count = %d, want 1", got)
	}
	if !f.HasConflicts() {
		t.Fatal("expected HasConflicts to be true")
	}

	c := f.Chunks[1]
	if c.Kind != KindConflict {
		t.Fatalf("chunk 1 kind = %v, want conflict", c.Kind)
	}
	if c.OursLabel != "HEAD" || c.TheirsLabel != "feature" {
		t.Fatalf("labels = %q/%q", c.OursLabel, c.TheirsLabel)
	}
	if strings.Join(c.Ours, "\n") != "main branch text" {
		t.Fatalf("ours = %q", c.Ours)
	}
	if strings.Join(c.Theirs, "\n") != "feature branch text" {
		t.Fatalf("theirs = %q", c.Theirs)
	}
}

func TestResolveStrategies(t *testing.T) {
	src := "before\n<<<<<<< HEAD\na\nb\n=======\nc\n>>>>>>> feature\nafter\n"
	f, _ := Parse("x", src)
	idx := f.ConflictChunkIndices()[0]

	f.Resolve(idx, ResolveOurs)
	if got := strings.Join(f.Chunks[idx].Result, "\n"); got != "a\nb" {
		t.Fatalf("ours result = %q", got)
	}

	f.Resolve(idx, ResolveTheirs)
	if got := strings.Join(f.Chunks[idx].Result, "\n"); got != "c" {
		t.Fatalf("theirs result = %q", got)
	}

	f.Resolve(idx, ResolveBoth)
	if got := strings.Join(f.Chunks[idx].Result, "\n"); got != "a\nb\nc" {
		t.Fatalf("both result = %q", got)
	}

	f.Resolve(idx, ResolveNone)
	if got := len(f.Chunks[idx].Result); got != 0 {
		t.Fatalf("none result length = %d, want 0", got)
	}
}

func TestRenderPreservesNonConflictLines(t *testing.T) {
	src := "line before\n<<<<<<< HEAD\nmain\n=======\nfeature\n>>>>>>> feature\nline after\n"
	f, _ := Parse("x", src)
	idx := f.ConflictChunkIndices()[0]

	// Unresolved render must reproduce the markers.
	got := f.Render()
	if got != src {
		t.Fatalf("unresolved render mismatch:\n got: %q\nwant: %q", got, src)
	}

	f.Resolve(idx, ResolveOurs)
	got = f.Render()
	want := "line before\nmain\nline after\n"
	if got != want {
		t.Fatalf("resolved render:\n got: %q\nwant: %q", got, want)
	}
}

func TestParseMultipleConflicts(t *testing.T) {
	src := "a\n<<<<<<< HEAD\n1\n=======\n2\n>>>>>>> f\nb\n<<<<<<< HEAD\n3\n=======\n4\n>>>>>>> f\nc\n"
	f, _ := Parse("x", src)
	if got := f.ConflictCount(); got != 2 {
		t.Fatalf("conflict count = %d, want 2", got)
	}
	if got := len(f.ConflictChunkIndices()); got != 2 {
		t.Fatalf("indices len = %d", got)
	}
	if f.AllResolved() {
		t.Fatal("should not be all resolved initially")
	}
	f.ResolveAll(ResolveTheirs)
	if !f.AllResolved() {
		t.Fatal("expected all resolved after ResolveAll")
	}
}

func TestNoConflictFile(t *testing.T) {
	src := "plain\ncontent\n"
	f, _ := Parse("x", src)
	if f.HasConflicts() {
		t.Fatal("expected no conflicts")
	}
	if got := f.Render(); got != src {
		t.Fatalf("render = %q, want %q", got, src)
	}
}

func TestTrailingNewlinePreserved(t *testing.T) {
	src := "x\n"
	f, _ := Parse("x", src)
	if got := f.Render(); got != src {
		t.Fatalf("render = %q, want %q", got, src)
	}
}

func TestMarkerWithEightEqualsIgnored(t *testing.T) {
	// A line with 8 '=' signs is not a valid Git separator and must be kept
	// as plain content rather than terminating a conflict block.
	src := "========\n"
	f, _ := Parse("x", src)
	if f.HasConflicts() {
		t.Fatal("expected no conflicts for 8-equals line")
	}
	if got := f.Render(); got != src {
		t.Fatalf("render = %q, want %q", got, src)
	}
}
