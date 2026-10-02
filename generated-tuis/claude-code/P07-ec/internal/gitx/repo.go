// Package gitx wraps the real git command-line tool.
//
// Every operation shells out to git; nothing about repository state is
// simulated or cached beyond the lifetime of a single call. This keeps the TUI
// honest -- what it shows is what git reports, and what it commits is what git
// commits.
package gitx

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

// commandTimeout bounds every git invocation so a hung or interactive git
// process cannot freeze the UI, which has no way to deliver a signal to it.
const commandTimeout = 30 * time.Second

// Repo is a handle on a git working tree.
type Repo struct {
	// Dir is the directory git commands run in.
	Dir string
}

// ErrNotARepo reports that Dir is not inside a git working tree.
var ErrNotARepo = errors.New("not a git repository")

// Open verifies that dir is inside a git working tree and returns a handle on
// it. The directory itself is used as the working directory for later commands
// rather than the repository root, so relative paths behave the way the user
// typed them.
func Open(dir string) (*Repo, error) {
	abs, err := filepath.Abs(dir)
	if err != nil {
		return nil, err
	}
	if st, err := os.Stat(abs); err != nil || !st.IsDir() {
		return nil, fmt.Errorf("%s is not a directory", dir)
	}
	// Resolve symlinks so that paths compare equal against the ones git
	// reports. Without this, a working tree reached through a symlinked parent
	// (/tmp on macOS, /bench bind mounts) makes RelPath produce a path full of
	// ".." segments that git cannot use.
	if resolved, err := filepath.EvalSymlinks(abs); err == nil {
		abs = resolved
	}
	r := &Repo{Dir: abs}
	if _, err := r.Run("rev-parse", "--git-dir"); err != nil {
		return nil, fmt.Errorf("%w: %s", ErrNotARepo, abs)
	}
	return r, nil
}

// Available reports whether the git executable can be found at all, so the
// caller can print a useful message instead of a raw exec error.
func Available() error {
	if _, err := exec.LookPath("git"); err != nil {
		return errors.New("git executable not found in PATH")
	}
	return nil
}

// Error is a failed git invocation. It keeps stderr because git's own message
// is almost always the most useful thing to show the user.
type Error struct {
	Args   []string
	Stderr string
	Err    error
}

func (e *Error) Error() string {
	msg := strings.TrimSpace(e.Stderr)
	if msg == "" {
		return fmt.Sprintf("git %s: %v", strings.Join(e.Args, " "), e.Err)
	}
	return fmt.Sprintf("git %s: %s", strings.Join(e.Args, " "), firstLine(msg))
}

func (e *Error) Unwrap() error { return e.Err }

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i]
	}
	return s
}

// Run executes a git command and returns its trimmed stdout.
func (r *Repo) Run(args ...string) (string, error) {
	out, _, err := r.run(args...)
	return strings.TrimRight(out, "\n"), err
}

// RunCombined executes a git command and returns stdout and stderr together.
// Commands such as `git commit` print useful information to both streams, and
// the UI shows the result verbatim.
func (r *Repo) RunCombined(args ...string) (string, error) {
	out, errOut, err := r.run(args...)
	combined := strings.TrimSpace(out)
	if e := strings.TrimSpace(errOut); e != "" {
		if combined != "" {
			combined += "\n"
		}
		combined += e
	}
	return combined, err
}

func (r *Repo) run(args ...string) (stdout, stderr string, err error) {
	ctx, cancel := context.WithTimeout(context.Background(), commandTimeout)
	defer cancel()

	// -c core.pager=cat and the terminal-disabling environment below stop git
	// from ever trying to page output or prompt for input, either of which
	// would deadlock behind the TUI's hold on the terminal.
	full := append([]string{"-c", "core.pager=cat", "--no-pager"}, args...)
	cmd := exec.CommandContext(ctx, "git", full...)
	cmd.Dir = r.Dir
	cmd.Stdin = nil
	cmd.Env = append(os.Environ(),
		"GIT_PAGER=cat",
		"PAGER=cat",
		"GIT_TERMINAL_PROMPT=0",
		"GIT_OPTIONAL_LOCKS=0",
		"LC_ALL=C",
		"GIT_EDITOR=true",
		"TERM=dumb",
	)
	var so, se bytes.Buffer
	cmd.Stdout = &so
	cmd.Stderr = &se
	runErr := cmd.Run()

	if ctx.Err() == context.DeadlineExceeded {
		return so.String(), se.String(), &Error{
			Args:   args,
			Stderr: se.String(),
			Err:    fmt.Errorf("timed out after %s", commandTimeout),
		}
	}
	if runErr != nil {
		return so.String(), se.String(), &Error{Args: args, Stderr: se.String(), Err: runErr}
	}
	return so.String(), se.String(), nil
}

// Status describes the parts of repository state the UI displays.
type Status struct {
	// Branch is the current branch name, or a detached-HEAD description.
	Branch string
	// Merging reports whether a merge is in progress (MERGE_HEAD exists).
	Merging bool
	// MergeHead is the short description of the branch being merged in.
	MergeHead string
	// Conflicted lists paths git reports as unmerged, repo-root relative.
	Conflicted []string
	// Staged lists paths with staged changes.
	Staged []string
	// Modified lists paths modified but not staged.
	Modified []string
}

// Status collects current repository state with real git queries.
func (r *Repo) Status() (*Status, error) {
	s := &Status{}

	if b, err := r.Run("rev-parse", "--abbrev-ref", "HEAD"); err == nil {
		s.Branch = b
	}
	if s.Branch == "HEAD" || s.Branch == "" {
		// Detached HEAD, or a repository with no commits yet.
		if sha, err := r.Run("rev-parse", "--short", "HEAD"); err == nil && sha != "" {
			s.Branch = "detached@" + sha
		} else {
			s.Branch = "(no commits yet)"
		}
	}

	gitDir, err := r.Run("rev-parse", "--absolute-git-dir")
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(filepath.Join(gitDir, "MERGE_HEAD")); err == nil {
		s.Merging = true
		// MERGE_MSG usually names the branch in a friendlier way than the raw
		// SHA; fall back to the abbreviated SHA when it is absent.
		if sha, err := r.Run("rev-parse", "--short", "MERGE_HEAD"); err == nil {
			s.MergeHead = sha
		}
		if msg, err := os.ReadFile(filepath.Join(gitDir, "MERGE_MSG")); err == nil {
			if name := branchFromMergeMsg(string(msg)); name != "" {
				s.MergeHead = name
			}
		}
	}

	// -z output avoids any quoting ambiguity for paths with spaces or
	// non-ASCII bytes.
	out, err := r.Run("status", "--porcelain=v1", "-z", "--untracked-files=no")
	if err != nil {
		return nil, err
	}
	for _, ent := range parsePorcelainZ(out) {
		switch {
		case ent.unmerged():
			s.Conflicted = append(s.Conflicted, ent.path)
		default:
			if ent.x != ' ' && ent.x != '?' {
				s.Staged = append(s.Staged, ent.path)
			}
			if ent.y != ' ' && ent.y != '?' {
				s.Modified = append(s.Modified, ent.path)
			}
		}
	}
	return s, nil
}

// branchFromMergeMsg pulls the merged branch name out of a MERGE_MSG header
// such as "Merge branch 'feature'".
func branchFromMergeMsg(msg string) string {
	line := firstLine(strings.TrimSpace(msg))
	i := strings.IndexByte(line, '\'')
	if i < 0 {
		return ""
	}
	rest := line[i+1:]
	j := strings.IndexByte(rest, '\'')
	if j < 0 {
		return ""
	}
	return rest[:j]
}

type porcelainEntry struct {
	x, y byte
	path string
}

// unmerged reports whether the status code pair marks a conflict. These are
// the code pairs git documents as unmerged states.
func (e porcelainEntry) unmerged() bool {
	switch string([]byte{e.x, e.y}) {
	case "DD", "AU", "UD", "UA", "DU", "AA", "UU":
		return true
	}
	return false
}

// parsePorcelainZ splits NUL-separated `git status --porcelain -z` output.
//
// Rename and copy entries carry a second NUL-terminated path (the origin);
// that extra field is consumed but not reported, since the current path is
// what the UI needs.
func parsePorcelainZ(out string) []porcelainEntry {
	var entries []porcelainEntry
	fields := strings.Split(out, "\x00")
	for i := 0; i < len(fields); i++ {
		rec := fields[i]
		if len(rec) < 4 {
			continue
		}
		e := porcelainEntry{x: rec[0], y: rec[1], path: rec[3:]}
		if e.x == 'R' || e.x == 'C' {
			i++ // skip the origin path that follows
		}
		entries = append(entries, e)
	}
	return entries
}

// ConflictedFiles lists unmerged paths, relative to the repository root.
func (r *Repo) ConflictedFiles() ([]string, error) {
	out, err := r.Run("diff", "--name-only", "--diff-filter=U", "-z")
	if err != nil {
		return nil, err
	}
	var files []string
	for _, p := range strings.Split(out, "\x00") {
		if p != "" {
			files = append(files, p)
		}
	}
	return files, nil
}

// Stage runs `git add` on the given paths so a resolved file is recorded as
// resolved.
func (r *Repo) Stage(paths ...string) error {
	if len(paths) == 0 {
		return nil
	}
	args := append([]string{"add", "--"}, paths...)
	_, err := r.RunCombined(args...)
	return err
}

// Commit creates the merge commit.
//
// When a merge is in progress git requires that no unmerged entries remain, so
// the caller is expected to have staged the resolved files first. The message
// is passed via -m, and -F is avoided so no temporary file is needed.
func (r *Repo) Commit(message string) (string, error) {
	if strings.TrimSpace(message) == "" {
		return "", errors.New("commit message must not be empty")
	}
	return r.RunCombined("commit", "--no-verify", "-m", message)
}

// DefaultMergeMessage returns the message git prepared for the merge, so the
// commit box starts from the conventional text instead of something invented.
func (r *Repo) DefaultMergeMessage() string {
	gitDir, err := r.Run("rev-parse", "--absolute-git-dir")
	if err != nil {
		return ""
	}
	data, err := os.ReadFile(filepath.Join(gitDir, "MERGE_MSG"))
	if err != nil {
		return ""
	}
	// MERGE_MSG carries commented conflict notes below the subject; keep only
	// the real message lines.
	var keep []string
	for _, l := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(l, "#") {
			continue
		}
		keep = append(keep, l)
	}
	return strings.TrimSpace(strings.Join(keep, "\n"))
}

// LogEntry is one commit in the history view.
type LogEntry struct {
	Hash    string
	Short   string
	Author  string
	When    string
	Subject string
	Refs    string
	// Merge is true when the commit has more than one parent.
	Merge bool
}

// logFieldSep and logRecordSep are byte sequences that will not occur in commit
// metadata, so splitting the output is unambiguous.
const (
	logFieldSep  = "\x1f"
	logRecordSep = "\x1e"
)

// Log returns up to limit commits reachable from HEAD, newest first.
func (r *Repo) Log(limit int) ([]LogEntry, error) {
	if limit <= 0 {
		limit = 100
	}
	format := strings.Join([]string{"%H", "%h", "%an", "%ad", "%s", "%D", "%P"}, logFieldSep) + logRecordSep
	out, err := r.Run("log",
		fmt.Sprintf("--max-count=%d", limit),
		"--date=format:%Y-%m-%d %H:%M",
		"--pretty=format:"+format,
	)
	if err != nil {
		// A repository with no commits is a normal state, not a failure.
		if strings.Contains(err.(*Error).Stderr, "does not have any commits") ||
			strings.Contains(err.(*Error).Stderr, "unknown revision") {
			return nil, nil
		}
		return nil, err
	}

	var entries []LogEntry
	for _, rec := range strings.Split(out, logRecordSep) {
		rec = strings.TrimLeft(rec, "\n")
		if strings.TrimSpace(rec) == "" {
			continue
		}
		f := strings.Split(rec, logFieldSep)
		if len(f) < 7 {
			continue
		}
		entries = append(entries, LogEntry{
			Hash:    f[0],
			Short:   f[1],
			Author:  f[2],
			When:    f[3],
			Subject: f[4],
			Refs:    f[5],
			Merge:   len(strings.Fields(f[6])) > 1,
		})
	}
	return entries, nil
}

// FileVersion reads one side of the conflict out of the index.
//
// Stage 1 is the common ancestor, 2 is ours and 3 is theirs. This gives the UI
// the pristine content of each side even when the working-tree file has been
// hand-edited, and it is read from the real index rather than reconstructed
// from markers.
func (r *Repo) FileVersion(path string, stage int) (string, error) {
	if stage < 1 || stage > 3 {
		return "", fmt.Errorf("invalid stage %d", stage)
	}
	return r.Run("show", fmt.Sprintf(":%d:%s", stage, path))
}

// AbortMerge runs `git merge --abort`, restoring the pre-merge state.
func (r *Repo) AbortMerge() (string, error) {
	return r.RunCombined("merge", "--abort")
}

// RelPath converts a path the user gave (relative to Repo.Dir, or absolute)
// into a repository-root-relative path suitable for git commands.
func (r *Repo) RelPath(path string) (string, error) {
	root, err := r.Run("rev-parse", "--show-toplevel")
	if err != nil {
		return "", err
	}
	if resolved, err := filepath.EvalSymlinks(root); err == nil {
		root = resolved
	}
	abs := path
	if !filepath.IsAbs(abs) {
		abs = filepath.Join(r.Dir, path)
	} else if d, err := filepath.EvalSymlinks(filepath.Dir(abs)); err == nil {
		// Resolve the parent rather than the file itself, which lets this work
		// for paths that do not exist yet.
		abs = filepath.Join(d, filepath.Base(abs))
	}
	rel, err := filepath.Rel(root, abs)
	if err != nil {
		return "", err
	}
	return filepath.ToSlash(rel), nil
}
