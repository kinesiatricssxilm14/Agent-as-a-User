// Package gitx provides thin wrappers around the real git command line tool.
// Every operation shells out to git, so the tool reflects genuine repository
// state and never simulates anything.
package gitx

import (
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// run executes a command in dir and returns trimmed combined output.
func run(dir string, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	out, err := cmd.CombinedOutput()
	trimmed := strings.TrimRight(string(out), "\n")
	if err != nil {
		return trimmed, err
	}
	return trimmed, nil
}

// FindRoot resolves the top-level directory of the Git repository containing
// dir. The returned path is absolute.
func FindRoot(dir string) (string, error) {
	out, err := run(dir, "git", "rev-parse", "--show-toplevel")
	if err != nil {
		return "", err
	}
	if out == "" {
		return "", errors.New("not inside a git repository")
	}
	if !filepath.IsAbs(out) {
		abs, err := filepath.Abs(filepath.Join(dir, out))
		if err != nil {
			return "", err
		}
		return abs, nil
	}
	return out, nil
}

// ConflictedFiles returns the paths (relative to the repository root) of all
// files that are currently in an unmerged / conflicted state.
func ConflictedFiles(repo string) ([]string, error) {
	out, err := run(repo, "git", "diff", "--name-only", "--diff-filter=U")
	if err != nil {
		return nil, err
	}
	if out == "" {
		return []string{}, nil
	}
	return strings.Split(out, "\n"), nil
}

// IsMerge reports whether a merge is currently in progress (MERGE_HEAD exists).
func IsMerge(repo string) bool {
	_, err := os.Stat(filepath.Join(repo, ".git", "MERGE_HEAD"))
	return err == nil
}

// Stage stages the given paths with "git add".
func Stage(repo string, files ...string) (string, error) {
	args := append([]string{"add", "--"}, files...)
	return run(repo, "git", args...)
}

// Commit creates a commit. When message is non-empty it is used verbatim;
// otherwise, if noEdit is true, "git commit --no-edit" is used (which consumes
// the merge message prepared by git during a merge).
func Commit(repo, message string, noEdit bool) (string, error) {
	if message != "" {
		return run(repo, "git", "commit", "-m", message)
	}
	if noEdit {
		return run(repo, "git", "commit", "--no-edit")
	}
	return "", errors.New("commit message is empty")
}

// Log returns the most recent n commits as "oneline" entries.
func Log(repo string, n int) (string, error) {
	return run(repo, "git", "log", "--oneline", "-n", itoa(n))
}

// configValue returns a git config value, or "" when unset.
func configValue(repo, key string) string {
	out, err := run(repo, "git", "config", key)
	if err != nil {
		return ""
	}
	return out
}

// EnsureIdentity makes sure the repository has a committer identity, because
// the tool frequently runs as root inside containers where none is configured.
// It only writes local (repository-scoped) config and never overrides an
// existing identity.
func EnsureIdentity(repo string) error {
	if configValue(repo, "user.name") != "" && configValue(repo, "user.email") != "" {
		return nil
	}
	if configValue(repo, "user.name") == "" {
		if _, err := run(repo, "git", "config", "user.name", "toolg"); err != nil {
			return err
		}
	}
	if configValue(repo, "user.email") == "" {
		if _, err := run(repo, "git", "config", "user.email", "toolg@localhost"); err != nil {
			return err
		}
	}
	return nil
}

// Status returns porcelain v1 status lines for the repository.
func Status(repo string) (string, error) {
	return run(repo, "git", "status", "--short")
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var b [20]byte
	i := len(b)
	neg := n < 0
	if neg {
		n = -n
	}
	for n > 0 {
		i--
		b[i] = byte('0' + n%10)
		n /= 10
	}
	if neg {
		i--
		b[i] = '-'
	}
	return string(b[i:])
}
