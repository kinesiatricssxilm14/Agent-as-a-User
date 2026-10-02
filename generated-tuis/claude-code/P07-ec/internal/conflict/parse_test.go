package conflict

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const sample = `line before
<<<<<<< HEAD
main branch text
=======
feature branch text
>>>>>>> feature
line after
`

func parseString(t *testing.T, s string) *File {
	t.Helper()
	f, err := ParseBytes("test", []byte(s))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	return f
}

func TestParseSample(t *testing.T) {
	f := parseString(t, sample)
	if len(f.Blocks) != 1 {
		t.Fatalf("want 1 block, got %d", len(f.Blocks))
	}
	b := f.Blocks[0]
	if got := strings.Join(b.Ours, "|"); got != "main branch text" {
		t.Errorf("ours = %q", got)
	}
	if got := strings.Join(b.Theirs, "|"); got != "feature branch text" {
		t.Errorf("theirs = %q", got)
	}
	if b.OursLabel != "HEAD" || b.TheirsLabel != "feature" {
		t.Errorf("labels = %q / %q", b.OursLabel, b.TheirsLabel)
	}
	if b.StartLine != 2 {
		t.Errorf("StartLine = %d, want 2", b.StartLine)
	}
	if b.HasBase {
		t.Error("HasBase should be false for 2-way markers")
	}
}

// The round trip through an unresolved render must be byte-identical, which is
// what makes a partial save safe.
func TestUnresolvedRoundTrip(t *testing.T) {
	f := parseString(t, sample)
	if got := string(f.Render()); got != sample {
		t.Errorf("round trip mismatch:\n%q", got)
	}
}

func TestChoices(t *testing.T) {
	cases := []struct {
		choice Choice
		want   string
	}{
		{ChoiceOurs, "line before\nmain branch text\nline after\n"},
		{ChoiceTheirs, "line before\nfeature branch text\nline after\n"},
		{ChoiceBoth, "line before\nmain branch text\nfeature branch text\nline after\n"},
		{ChoiceNone, "line before\nline after\n"},
	}
	for _, tc := range cases {
		t.Run(tc.choice.String(), func(t *testing.T) {
			f := parseString(t, sample)
			f.Blocks[0].Choice = tc.choice
			got := string(f.Render())
			if got != tc.want {
				t.Errorf("got:\n%q\nwant:\n%q", got, tc.want)
			}
			if strings.ContainsAny(got, "<>") || strings.Contains(got, "=======") {
				t.Errorf("markers survived resolution: %q", got)
			}
		})
	}
}

func TestCustomChoice(t *testing.T) {
	f := parseString(t, sample)
	f.Blocks[0].Choice = ChoiceCustom
	f.Blocks[0].Custom = []string{"merged by hand"}
	want := "line before\nmerged by hand\nline after\n"
	if got := string(f.Render()); got != want {
		t.Errorf("got %q want %q", got, want)
	}
}

func TestMultipleBlocksPreserveInterleavedText(t *testing.T) {
	src := "a\n<<<<<<< HEAD\no1\n=======\nt1\n>>>>>>> f\nb\nc\n<<<<<<< HEAD\no2\n=======\nt2\n>>>>>>> f\nd\n"
	f := parseString(t, src)
	if len(f.Blocks) != 2 {
		t.Fatalf("want 2 blocks, got %d", len(f.Blocks))
	}
	f.Blocks[0].Choice = ChoiceOurs
	f.Blocks[1].Choice = ChoiceTheirs
	want := "a\no1\nb\nc\nt2\nd\n"
	if got := string(f.Render()); got != want {
		t.Errorf("got %q want %q", got, want)
	}
}

// Resolving one block must not disturb the other, and the untouched one keeps
// its markers so the file stays a valid conflict file.
func TestPartialResolution(t *testing.T) {
	src := "a\n<<<<<<< HEAD\no1\n=======\nt1\n>>>>>>> f\nb\n<<<<<<< HEAD\no2\n=======\nt2\n>>>>>>> f\nc\n"
	f := parseString(t, src)
	f.Blocks[0].Choice = ChoiceOurs
	got := string(f.Render())
	want := "a\no1\nb\n<<<<<<< HEAD\no2\n=======\nt2\n>>>>>>> f\nc\n"
	if got != want {
		t.Errorf("got %q want %q", got, want)
	}
	if f.UnresolvedCount() != 1 {
		t.Errorf("UnresolvedCount = %d, want 1", f.UnresolvedCount())
	}
}

func TestDiff3Base(t *testing.T) {
	src := "x\n<<<<<<< HEAD\nours\n||||||| base\nancestor\n=======\ntheirs\n>>>>>>> other\ny\n"
	f := parseString(t, src)
	b := f.Blocks[0]
	if !b.HasBase {
		t.Fatal("HasBase = false")
	}
	if strings.Join(b.Base, "|") != "ancestor" {
		t.Errorf("base = %v", b.Base)
	}
	if b.BaseLabel != "base" {
		t.Errorf("BaseLabel = %q", b.BaseLabel)
	}
	if got := string(f.Render()); got != src {
		t.Errorf("diff3 round trip mismatch: %q", got)
	}
	// The ancestor section must never leak into a resolution.
	b.Choice = ChoiceBoth
	if got := string(f.Render()); strings.Contains(got, "ancestor") {
		t.Errorf("base leaked into result: %q", got)
	}
}

func TestNoConflicts(t *testing.T) {
	f := parseString(t, "just\nplain\ntext\n")
	if f.HasConflicts() {
		t.Error("HasConflicts = true")
	}
	if got := string(f.Render()); got != "just\nplain\ntext\n" {
		t.Errorf("got %q", got)
	}
}

func TestEmptySides(t *testing.T) {
	// A pure addition on one side yields an empty opposite side.
	f := parseString(t, "a\n<<<<<<< HEAD\n=======\nnew\n>>>>>>> f\nb\n")
	b := f.Blocks[0]
	if len(b.Ours) != 0 {
		t.Errorf("ours = %v, want empty", b.Ours)
	}
	b.Choice = ChoiceOurs
	if got := string(f.Render()); got != "a\nb\n" {
		t.Errorf("got %q", got)
	}
}

func TestNoTrailingNewline(t *testing.T) {
	src := "a\n<<<<<<< HEAD\no\n=======\nt\n>>>>>>> f\nlast"
	f := parseString(t, src)
	f.Blocks[0].Choice = ChoiceOurs
	if got := string(f.Render()); got != "a\no\nlast" {
		t.Errorf("got %q", got)
	}
}

func TestCRLFPreserved(t *testing.T) {
	src := "a\r\n<<<<<<< HEAD\r\no\r\n=======\r\nt\r\n>>>>>>> f\r\nb\r\n"
	f := parseString(t, src)
	f.Blocks[0].Choice = ChoiceTheirs
	if got := string(f.Render()); got != "a\r\nt\r\nb\r\n" {
		t.Errorf("got %q", got)
	}
}

// Marker-like text that is not a real marker (no seven characters, or appearing
// where the grammar does not allow it) must be treated as content.
func TestMarkerLookalikesAreContent(t *testing.T) {
	src := "<<< not a marker\n=== also not\n>>> nope\n"
	f := parseString(t, src)
	if f.HasConflicts() {
		t.Error("lookalikes parsed as a conflict")
	}
	if got := string(f.Render()); got != src {
		t.Errorf("got %q", got)
	}
}

func TestMalformedErrors(t *testing.T) {
	cases := map[string]string{
		"unterminated":   "a\n<<<<<<< HEAD\no\n=======\nt\n",
		"nested opening": "<<<<<<< A\no\n<<<<<<< B\n=======\nt\n>>>>>>> C\n",
		"no separator":   "<<<<<<< A\no\n>>>>>>> B\n",
	}
	for name, src := range cases {
		t.Run(name, func(t *testing.T) {
			if _, err := ParseBytes("t", []byte(src)); err == nil {
				t.Error("want error, got nil")
			}
		})
	}
}

func TestResolveAllAndResultBlockRange(t *testing.T) {
	src := "a\n<<<<<<< HEAD\no1\no1b\n=======\nt1\n>>>>>>> f\nb\n<<<<<<< HEAD\no2\n=======\nt2\n>>>>>>> f\n"
	f := parseString(t, src)
	f.ResolveAll(ChoiceOurs)
	if f.UnresolvedCount() != 0 {
		t.Fatalf("UnresolvedCount = %d", f.UnresolvedCount())
	}
	// Block 0 contributes 2 lines starting after "a".
	start, count, ok := f.ResultBlockRange(0)
	if !ok || start != 1 || count != 2 {
		t.Errorf("range(0) = %d,%d,%v want 1,2,true", start, count, ok)
	}
	// Block 1 sits after "a", 2 ours lines and "b".
	start, count, ok = f.ResultBlockRange(1)
	if !ok || start != 4 || count != 1 {
		t.Errorf("range(1) = %d,%d,%v want 4,1,true", start, count, ok)
	}
	if _, _, ok := f.ResultBlockRange(9); ok {
		t.Error("out-of-range block reported ok")
	}
}

// A block resolved to nothing still needs a well-defined position so the UI can
// point at where it used to be.
func TestResultBlockRangeEmpty(t *testing.T) {
	f := parseString(t, sample)
	f.Blocks[0].Choice = ChoiceNone
	start, count, ok := f.ResultBlockRange(0)
	if !ok || start != 1 || count != 0 {
		t.Errorf("got %d,%d,%v want 1,0,true", start, count, ok)
	}
}

func TestSaveAtomicAndPreservesMode(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "conflict.py")
	if err := os.WriteFile(path, []byte(sample), 0o755); err != nil {
		t.Fatal(err)
	}
	f, mode, err := Load(path)
	if err != nil {
		t.Fatal(err)
	}
	if mode != 0o755 {
		t.Errorf("mode = %o", mode)
	}
	f.Blocks[0].Choice = ChoiceTheirs
	if err := f.Save(path, mode); err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	want := "line before\nfeature branch text\nline after\n"
	if string(got) != want {
		t.Errorf("on disk: %q want %q", got, want)
	}
	st, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if st.Mode().Perm() != 0o755 {
		t.Errorf("mode after save = %o", st.Mode().Perm())
	}
	// No temporary files may be left behind.
	ents, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(ents) != 1 {
		t.Errorf("leftover files: %v", ents)
	}
}

func TestLoadMissingFile(t *testing.T) {
	if _, _, err := Load(filepath.Join(t.TempDir(), "nope")); err == nil {
		t.Error("want error for missing file")
	}
}

func TestLoadDirectory(t *testing.T) {
	if _, _, err := Load(t.TempDir()); err == nil {
		t.Error("want error for directory")
	}
}
