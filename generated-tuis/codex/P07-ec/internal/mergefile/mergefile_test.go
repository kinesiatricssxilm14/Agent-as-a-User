package mergefile

import (
	"strings"
	"testing"
)

const sample = "before\n<<<<<<< HEAD\nours one\nours two\n=======\ntheirs\n>>>>>>> feature\nafter\n"

func TestParseAndStrategies(t *testing.T) {
	for _, tc := range []struct {
		choice Choice
		want   string
	}{
		{Ours, "before\nours one\nours two\nafter\n"},
		{Theirs, "before\ntheirs\nafter\n"},
		{Both, "before\nours one\nours two\ntheirs\nafter\n"},
		{None, "before\nafter\n"},
	} {
		f, err := Parse([]byte(sample))
		if err != nil {
			t.Fatal(err)
		}
		if len(f.Conflicts) != 1 {
			t.Fatalf("got %d conflicts", len(f.Conflicts))
		}
		f.Conflicts[0].Choice = tc.choice
		got := f.Render(false)
		if got != tc.want {
			t.Errorf("%s:\ngot  %q\nwant %q", tc.choice, got, tc.want)
		}
		if strings.Contains(got, "<<<<<<<") || strings.Contains(got, "=======") || strings.Contains(got, ">>>>>>>") {
			t.Errorf("markers remain for %s", tc.choice)
		}
	}
}

func TestMultipleConflictsAndDiff3(t *testing.T) {
	input := "top\n<<<<<<< ours\na\n||||||| base\nold\n=======\nb\n>>>>>>> theirs\nmiddle\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> topic\nbottom"
	f, err := Parse([]byte(input))
	if err != nil {
		t.Fatal(err)
	}
	if len(f.Conflicts) != 2 {
		t.Fatalf("got %d conflicts", len(f.Conflicts))
	}
	if got := strings.Join(f.Conflicts[0].Base, ""); got != "old\n" {
		t.Fatalf("base = %q", got)
	}
	f.Conflicts[0].Choice = Theirs
	f.Conflicts[1].Choice = Ours
	want := "top\nb\nmiddle\nx\nbottom"
	if got := f.Render(false); got != want {
		t.Fatalf("got %q want %q", got, want)
	}
}

func TestMalformed(t *testing.T) {
	if _, err := Parse([]byte("<<<<<<< HEAD\na\n=======\nb\n")); err == nil {
		t.Fatal("expected malformed marker error")
	}
}
