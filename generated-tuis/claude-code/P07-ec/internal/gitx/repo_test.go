package gitx

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// newConflictRepo builds a real repository whose merge is stopped on a
// conflict in conflict.py, mirroring the runtime scenario.
func newConflictRepo(t *testing.T) string {
	t.Helper()
	if err := Available(); err != nil {
		t.Skip("git not available")
	}
	dir := t.TempDir()

	run := func(args ...string) string {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=anonymous@example.invalid",
			"GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=anonymous@example.invalid",
			"GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_SYSTEM=/dev/null",
			"LC_ALL=C",
		)
		out, err := cmd.CombinedOutput()
		// A conflicting merge exits non-zero by design.
		if err != nil && args[0] != "merge" {
			t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
		}
		return string(out)
	}

	write := func(name, content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	run("init", "-q", "-b", "main")
	run("config", "user.name", "Test")
	run("config", "user.email", "anonymous@example.invalid")
	run("config", "commit.gpgsign", "false")

	write("conflict.py", "line before\nshared = 1\nline after\n")
	write("stable.txt", "untouched\n")
	run("add", ".")
	run("commit", "-q", "-m", "initial commit")

	run("checkout", "-q", "-b", "feature")
	write("conflict.py", "line before\nfeature branch text\nline after\n")
	run("commit", "-q", "-am", "feature change")

	run("checkout", "-q", "main")
	write("conflict.py", "line before\nmain branch text\nline after\n")
	run("commit", "-q", "-am", "main change")

	run("merge", "feature") // conflicts on purpose
	return dir
}

func TestOpenRejectsNonRepo(t *testing.T) {
	if _, err := Open(t.TempDir()); err == nil {
		t.Error("want ErrNotARepo for a plain directory")
	}
	if _, err := Open(filepath.Join(t.TempDir(), "missing")); err == nil {
		t.Error("want error for a missing directory")
	}
}

func TestStatusDuringConflict(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	s, err := r.Status()
	if err != nil {
		t.Fatal(err)
	}
	if s.Branch != "main" {
		t.Errorf("Branch = %q, want main", s.Branch)
	}
	if !s.Merging {
		t.Error("Merging = false during a conflicted merge")
	}
	if s.MergeHead != "feature" {
		t.Errorf("MergeHead = %q, want feature", s.MergeHead)
	}
	if len(s.Conflicted) != 1 || s.Conflicted[0] != "conflict.py" {
		t.Errorf("Conflicted = %v, want [conflict.py]", s.Conflicted)
	}
}

func TestConflictedFiles(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	files, err := r.ConflictedFiles()
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 1 || files[0] != "conflict.py" {
		t.Errorf("got %v", files)
	}
}

// The working tree file must really contain markers -- this is what the parser
// consumes at runtime.
func TestWorkingTreeHasMarkers(t *testing.T) {
	dir := newConflictRepo(t)
	data, err := os.ReadFile(filepath.Join(dir, "conflict.py"))
	if err != nil {
		t.Fatal(err)
	}
	for _, m := range []string{"<<<<<<<", "=======", ">>>>>>>"} {
		if !strings.Contains(string(data), m) {
			t.Errorf("missing marker %q in:\n%s", m, data)
		}
	}
}

func TestFileVersionStages(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	ours, err := r.FileVersion("conflict.py", 2)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(ours, "main branch text") {
		t.Errorf("stage 2 (ours) = %q", ours)
	}
	theirs, err := r.FileVersion("conflict.py", 3)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(theirs, "feature branch text") {
		t.Errorf("stage 3 (theirs) = %q", theirs)
	}
	base, err := r.FileVersion("conflict.py", 1)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(base, "shared = 1") {
		t.Errorf("stage 1 (base) = %q", base)
	}
	if _, err := r.FileVersion("conflict.py", 7); err == nil {
		t.Error("want error for an invalid stage")
	}
}

// The full flow: resolve on disk, stage, commit, and confirm git agrees the
// merge is finished and the commit really has two parents.
func TestStageAndCommitCompletesMerge(t *testing.T) {
	dir := newConflictRepo(t)
	r, err := Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	resolved := "line before\nmain branch text\nfeature branch text\nline after\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(resolved), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := r.Stage("conflict.py"); err != nil {
		t.Fatal(err)
	}
	out, err := r.Commit("Merge branch 'feature'")
	if err != nil {
		t.Fatalf("commit failed: %v\n%s", err, out)
	}

	s, err := r.Status()
	if err != nil {
		t.Fatal(err)
	}
	if s.Merging {
		t.Error("still merging after commit")
	}
	if len(s.Conflicted) != 0 {
		t.Errorf("conflicts remain: %v", s.Conflicted)
	}

	log, err := r.Log(10)
	if err != nil {
		t.Fatal(err)
	}
	if len(log) == 0 {
		t.Fatal("empty log")
	}
	if !log[0].Merge {
		t.Error("HEAD is not a merge commit")
	}
	if log[0].Subject != "Merge branch 'feature'" {
		t.Errorf("subject = %q", log[0].Subject)
	}
	// The resolution must survive in the committed tree.
	got, err := r.Run("show", "HEAD:conflict.py")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(got, "feature branch text") || !strings.Contains(got, "main branch text") {
		t.Errorf("committed content = %q", got)
	}
}

// Committing with unmerged entries must fail rather than appear to succeed.
func TestCommitFailsWithUnresolvedConflicts(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := r.Commit("premature"); err == nil {
		t.Error("commit succeeded despite unmerged entries")
	}
}

func TestCommitRejectsEmptyMessage(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := r.Commit("   "); err == nil {
		t.Error("want error for a blank message")
	}
}

func TestDefaultMergeMessage(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	msg := r.DefaultMergeMessage()
	if !strings.Contains(msg, "feature") {
		t.Errorf("DefaultMergeMessage = %q, want it to mention the branch", msg)
	}
	if strings.Contains(msg, "#") {
		t.Errorf("comment lines leaked into the message: %q", msg)
	}
}

func TestLog(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	entries, err := r.Log(10)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) < 2 {
		t.Fatalf("want at least 2 commits, got %d", len(entries))
	}
	if entries[0].Subject != "main change" {
		t.Errorf("newest subject = %q", entries[0].Subject)
	}
	for _, e := range entries {
		if len(e.Hash) != 40 {
			t.Errorf("bad hash %q", e.Hash)
		}
		if e.Short == "" || e.Author == "" || e.When == "" {
			t.Errorf("incomplete entry %+v", e)
		}
	}
	if got, err := r.Log(1); err != nil || len(got) != 1 {
		t.Errorf("limit ignored: %d entries, err %v", len(got), err)
	}
}

func TestAbortMerge(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := r.AbortMerge(); err != nil {
		t.Fatal(err)
	}
	s, err := r.Status()
	if err != nil {
		t.Fatal(err)
	}
	if s.Merging {
		t.Error("still merging after abort")
	}
}

func TestRelPathFromSubdirectory(t *testing.T) {
	dir := newConflictRepo(t)
	sub := filepath.Join(dir, "pkg", "inner")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	r, err := Open(sub)
	if err != nil {
		t.Fatal(err)
	}
	got, err := r.RelPath("file.txt")
	if err != nil {
		t.Fatal(err)
	}
	if got != "pkg/inner/file.txt" {
		t.Errorf("RelPath = %q", got)
	}
	// An absolute path must resolve the same way.
	got, err = r.RelPath(filepath.Join(dir, "conflict.py"))
	if err != nil {
		t.Fatal(err)
	}
	if got != "conflict.py" {
		t.Errorf("RelPath(abs) = %q", got)
	}
}

func TestErrorMessageIncludesStderr(t *testing.T) {
	r, err := Open(newConflictRepo(t))
	if err != nil {
		t.Fatal(err)
	}
	_, err = r.Run("cat-file", "-p", "definitelynotarealref")
	if err == nil {
		t.Fatal("want an error")
	}
	var ge *Error
	if !asGitError(err, &ge) {
		t.Fatalf("want *gitx.Error, got %T", err)
	}
	if ge.Stderr == "" {
		t.Error("stderr not captured")
	}
	if !strings.Contains(err.Error(), "git ") {
		t.Errorf("message = %q", err.Error())
	}
}

func asGitError(err error, target **Error) bool {
	if e, ok := err.(*Error); ok {
		*target = e
		return true
	}
	return false
}

func TestParsePorcelainZ(t *testing.T) {
	// A rename entry carries an extra origin path that must be skipped so the
	// following entry is not misread.
	out := "UU conflict.py\x00R  new.txt\x00old.txt\x00 M mod.txt\x00"
	got := parsePorcelainZ(out)
	if len(got) != 3 {
		t.Fatalf("got %d entries: %+v", len(got), got)
	}
	if got[0].path != "conflict.py" || !got[0].unmerged() {
		t.Errorf("entry 0 = %+v", got[0])
	}
	if got[1].path != "new.txt" {
		t.Errorf("entry 1 = %+v", got[1])
	}
	if got[2].path != "mod.txt" || got[2].unmerged() {
		t.Errorf("entry 2 = %+v", got[2])
	}
}

func TestBranchFromMergeMsg(t *testing.T) {
	cases := map[string]string{
		"Merge branch 'feature'\n\n# notes":  "feature",
		"Merge branch 'topic/x' into main":   "topic/x",
		"Merge remote-tracking branch 'o/b'": "o/b",
		"no quotes here":                     "",
		"":                                   "",
	}
	for in, want := range cases {
		if got := branchFromMergeMsg(in); got != want {
			t.Errorf("branchFromMergeMsg(%q) = %q, want %q", in, got, want)
		}
	}
}
