package pkgmgr

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
)

// ExecError carries the exit status and captured output of a failed command.
type ExecError struct {
	Cmd    Command
	Err    error
	Stderr string
}

func (e *ExecError) Error() string {
	msg := strings.TrimSpace(e.Stderr)
	if msg == "" {
		return fmt.Sprintf("%s: %v", e.Cmd.Display(), e.Err)
	}
	// Keep the message compact: the full output is streamed to the log pane.
	if i := strings.LastIndexByte(msg, '\n'); i >= 0 {
		msg = strings.TrimSpace(msg[i+1:])
	}
	return fmt.Sprintf("%s: %v: %s", e.Cmd.Display(), e.Err, msg)
}

func (e *ExecError) Unwrap() error { return e.Err }

// run executes cmd and returns stdout. stderr is captured separately and only
// surfaced on failure; both streams are mirrored to the logger when one is set.
func run(ctx context.Context, log Logger, cmd Command) (string, error) {
	log.log("$ %s", cmd.Display())

	c := exec.CommandContext(ctx, cmd.Name, cmd.Args...)
	c.Env = commandEnv(cmd.Env)

	var stdout, stderr bytes.Buffer
	c.Stdout = &stdout
	c.Stderr = &stderr

	err := c.Run()
	if err != nil {
		if ctx.Err() != nil {
			log.log("! cancelled")
			return stdout.String(), ctx.Err()
		}
		logLines(log, stdout.String())
		logLines(log, stderr.String())
		return stdout.String(), &ExecError{Cmd: cmd, Err: err, Stderr: stderr.String()}
	}
	return stdout.String(), nil
}

// runStreaming executes cmd and forwards every output line to log as it
// arrives, so long installs report progress instead of going quiet.
func runStreaming(ctx context.Context, log Logger, cmd Command) error {
	log.log("$ %s", cmd.Display())

	c := exec.CommandContext(ctx, cmd.Name, cmd.Args...)
	c.Env = commandEnv(cmd.Env)

	stdout, err := c.StdoutPipe()
	if err != nil {
		return &ExecError{Cmd: cmd, Err: err}
	}
	stderr, err := c.StderrPipe()
	if err != nil {
		return &ExecError{Cmd: cmd, Err: err}
	}
	if err := c.Start(); err != nil {
		return &ExecError{Cmd: cmd, Err: err}
	}

	var (
		wg   sync.WaitGroup
		mu   sync.Mutex
		tail []string
	)
	pump := func(r io.Reader) {
		defer wg.Done()
		for line := range lines(r) {
			log.log("%s", line)
			mu.Lock()
			tail = append(tail, line)
			if len(tail) > 12 {
				tail = tail[len(tail)-12:]
			}
			mu.Unlock()
		}
	}
	wg.Add(2)
	go pump(stdout)
	go pump(stderr)
	wg.Wait()

	if err := c.Wait(); err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		mu.Lock()
		captured := strings.Join(tail, "\n")
		mu.Unlock()
		return &ExecError{Cmd: cmd, Err: err, Stderr: captured}
	}
	return nil
}

// RunPlan executes every step of p in order, stopping at the first error.
func RunPlan(ctx context.Context, log Logger, p Plan) error {
	if len(p.Steps) == 0 {
		return errors.New("nothing to do")
	}
	for i, step := range p.Steps {
		log.log("── step %d/%d ──", i+1, len(p.Steps))
		if err := runStreaming(ctx, log, step); err != nil {
			return err
		}
	}
	return nil
}

// commandEnv layers extra onto the inherited environment and forces
// non-interactive, unpaged, plain-text behaviour out of the package managers.
func commandEnv(extra []string) []string {
	env := append([]string{}, os.Environ()...)
	env = append(env,
		"DEBIAN_FRONTEND=noninteractive",
		"PYTHONUNBUFFERED=1",
		"PYTHONIOENCODING=utf-8",
		"PIP_DISABLE_PIP_VERSION_CHECK=1",
		"PIP_NO_INPUT=1",
		"PIP_PROGRESS_BAR=off",
		"COLUMNS=200",
		"LC_ALL=C.UTF-8",
		"PAGER=cat",
		"GIT_PAGER=cat",
		"TERM=dumb",
	)
	return append(env, extra...)
}

// lines yields the reader's content one line at a time with trailing carriage
// returns trimmed. Long lines are chunked rather than dropped.
func lines(r io.Reader) <-chan string {
	out := make(chan string, 32)
	go func() {
		defer close(out)
		var buf []byte
		chunk := make([]byte, 4096)
		flush := func() {
			text := strings.TrimRight(string(buf), "\r")
			buf = buf[:0]
			if strings.TrimSpace(text) != "" {
				out <- text
			}
		}
		for {
			n, err := r.Read(chunk)
			for _, b := range chunk[:n] {
				if b == '\n' {
					flush()
					continue
				}
				buf = append(buf, b)
				if len(buf) > 4000 {
					flush()
				}
			}
			if err != nil {
				flush()
				return
			}
		}
	}()
	return out
}

func logLines(log Logger, text string) {
	if log == nil {
		return
	}
	for _, line := range strings.Split(strings.TrimRight(text, "\n"), "\n") {
		if strings.TrimSpace(line) != "" {
			log(strings.TrimRight(line, "\r"))
		}
	}
}

// lookPath reports whether name is an executable on PATH.
func lookPath(name string) (string, bool) {
	p, err := exec.LookPath(name)
	if err != nil {
		return "", false
	}
	return p, true
}
