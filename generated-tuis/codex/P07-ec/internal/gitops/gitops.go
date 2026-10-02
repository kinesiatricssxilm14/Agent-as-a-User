package gitops

import (
	"bytes"
	"fmt"
	"os/exec"
	"strings"
)

type Repo struct{ Dir string }

func (r Repo) run(args ...string) (string, error) {
	cmd := exec.Command("git", append([]string{"-C", r.Dir}, args...)...)
	var out bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &out
	err := cmd.Run()
	return strings.TrimSpace(out.String()), err
}

func (r Repo) Root() (string, error) {
	out, err := r.run("rev-parse", "--show-toplevel")
	if err != nil {
		return "", fmt.Errorf("not a Git repository: %s", out)
	}
	return out, nil
}

func (r Repo) Add(path string) error {
	out, err := r.run("add", "--", path)
	if err != nil {
		return fmt.Errorf("git add: %s", out)
	}
	return nil
}

func (r Repo) Commit(message string) (string, error) {
	out, err := r.run("commit", "-m", message)
	if err != nil {
		return out, fmt.Errorf("git commit: %s", out)
	}
	return out, nil
}

func (r Repo) History(n int) string {
	out, err := r.run("log", fmt.Sprintf("-%d", n), "--graph", "--decorate", "--date=short", "--pretty=format:%h %ad %d %s")
	if err != nil {
		return "History unavailable: " + out
	}
	if out == "" {
		return "No commits yet."
	}
	return out
}

func (r Repo) Status(path string) string {
	out, err := r.run("status", "--short", "--", path)
	if err != nil {
		return "git status unavailable"
	}
	if out == "" {
		return "clean"
	}
	return out
}
