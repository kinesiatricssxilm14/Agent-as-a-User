package tui

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"toolg/internal/conflict"
	"toolg/internal/gitx"
)

func keyRune(r rune) tea.KeyMsg        { return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}} }
func keyType(k tea.KeyType) tea.KeyMsg { return tea.KeyMsg{Type: k} }

func step(t *testing.T, m Model, msg tea.Msg) Model {
	t.Helper()
	nm, _ := m.Update(msg)
	return nm.(Model)
}

// setupConflictRepo builds a small repository that ends in a merge conflict on
// conflict.py.
func setupConflictRepo(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	env := append(os.Environ(),
		"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=anonymous@example.invalid",
		"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=anonymous@example.invalid",
	)
	git := func(args ...string) string {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = env
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return string(out)
	}
	write := func(content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	git("init", "-q")
	write("import sys\n\ndef helper():\n    return 42\n\ndef main():\n    print(\"result:\", helper())\n")
	git("add", "conflict.py")
	git("commit", "-qm", "initial")

	git("checkout", "-qb", "feature")
	write("import sys\n\ndef helper():\n    return 42\n\ndef main():\n    print(\"feature result:\", helper())\n")
	git("add", "conflict.py")
	git("commit", "-qm", "feature change")

	git("checkout", "-q", "main")
	write("import sys\n\ndef helper():\n    return 42\n\ndef main():\n    print(\"main result:\", helper())\n")
	git("add", "conflict.py")
	git("commit", "-qm", "main change")

	merge := exec.Command("git", "merge", "feature")
	merge.Dir = dir
	merge.Env = env
	_ = merge.Run() // expected to conflict
	return dir
}

func TestNewOpensConflictInMergeView(t *testing.T) {
	dir := setupConflictRepo(t)
	m := New(dir, "conflict.py")
	m = step(t, m, tea.WindowSizeMsg{Width: 130, Height: 40})

	if m.screen != screenMerge {
		t.Fatalf("screen = %v, want merge", m.screen)
	}
	if m.cf == nil || !m.cf.HasConflicts() {
		t.Fatal("expected conflicts in opened file")
	}
	if m.cf.ConflictCount() != 1 {
		t.Fatalf("conflict count = %d, want 1", m.cf.ConflictCount())
	}
	if m.repo == "" {
		t.Fatal("repo root not discovered")
	}
	if m.relPath != "conflict.py" {
		t.Fatalf("relPath = %q", m.relPath)
	}

	view := m.View()
	for _, want := range []string{"OURS", "RESULT", "THEIRS", "main result", "feature result"} {
		if !strings.Contains(view, want) {
			t.Errorf("view missing %q", want)
		}
	}
}

func TestResolutionSaveAndCommitFlow(t *testing.T) {
	dir := setupConflictRepo(t)
	m := New(dir, "conflict.py")
	m = step(t, m, tea.WindowSizeMsg{Width: 130, Height: 40})

	// Resolve the current conflict as "ours".
	m = step(t, m, keyRune('1'))
	if got := m.currentResolution(); got != conflict.ResolveOurs {
		t.Fatalf("resolution = %v, want ours", got)
	}

	// Save writes the resolved content and strips markers.
	m = step(t, m, keyRune('s'))
	data, err := os.ReadFile(filepath.Join(dir, "conflict.py"))
	if err != nil {
		t.Fatal(err)
	}
	s := string(data)
	if strings.Contains(s, "<<<<<<<") || strings.Contains(s, ">>>>>>>") {
		t.Fatalf("markers not removed after save:\n%s", s)
	}
	if !strings.Contains(s, "main result") {
		t.Fatalf("ours content missing after save:\n%s", s)
	}
	if strings.Contains(s, "feature result") {
		t.Fatalf("theirs content should be gone after ours resolution:\n%s", s)
	}
	if !strings.Contains(s, "import sys") {
		t.Fatalf("non-conflict line lost:\n%s", s)
	}

	// Commit (empty message -> --no-edit since a merge is in progress).
	m = step(t, m, keyRune('c'))
	if m.screen != screenCommit {
		t.Fatalf("screen = %v, want commit", m.screen)
	}
	m = step(t, m, keyType(tea.KeyEnter))
	if m.commitErr != "" {
		t.Fatalf("commit failed: %s", m.commitErr)
	}
	if m.screen != screenMerge {
		t.Fatalf("after commit screen = %v, want merge", m.screen)
	}

	log, err := gitx.Log(dir, 10)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(log, "Merge branch 'feature'") {
		t.Fatalf("merge commit not found in log:\n%s", log)
	}

	// History screen shows the log.
	m = step(t, m, keyRune('g'))
	if m.screen != screenHistory {
		t.Fatalf("screen = %v, want history", m.screen)
	}
	if !strings.Contains(m.View(), "Merge branch 'feature'") {
		t.Fatalf("history view missing merge commit:\n%s", m.View())
	}
}

func TestFilesListScreen(t *testing.T) {
	dir := setupConflictRepo(t)
	m := New(dir, "")
	if m.screen != screenFiles {
		t.Fatalf("screen = %v, want files list", m.screen)
	}
	if len(m.files) != 1 || m.files[0] != "conflict.py" {
		t.Fatalf("files = %v", m.files)
	}
	// Enter opens the selected file.
	m = step(t, m, keyType(tea.KeyEnter))
	if m.screen != screenMerge {
		t.Fatalf("screen = %v, want merge", m.screen)
	}
}

func TestHelpScreenRoundTrip(t *testing.T) {
	dir := setupConflictRepo(t)
	m := New(dir, "conflict.py")
	m = step(t, m, keyRune('?'))
	if m.screen != screenHelp {
		t.Fatalf("screen = %v, want help", m.screen)
	}
	view := m.View()
	for _, want := range []string{"ours", "theirs", "both", "save", "commit"} {
		if !strings.Contains(view, want) {
			t.Errorf("help missing %q", want)
		}
	}
	m = step(t, m, keyRune('?'))
	if m.screen != screenMerge {
		t.Fatalf("back from help screen = %v, want merge", m.screen)
	}
}

func TestAllResolveShortcuts(t *testing.T) {
	dir := setupConflictRepo(t)
	// Build a two-conflict file directly on the model.
	m := New(dir, "conflict.py")
	m.cf.Resolve(0, conflict.ResolveOurs) // ensure at least one conflict resolved state
	m = step(t, m, keyRune('T'))          // resolve all as theirs
	if !m.cf.AllResolved() {
		t.Fatal("expected all resolved after 'T'")
	}
	if m.cf.Chunks[m.cf.ConflictChunkIndices()[0]].Resolution != conflict.ResolveTheirs {
		t.Fatal("expected theirs resolution")
	}
}
