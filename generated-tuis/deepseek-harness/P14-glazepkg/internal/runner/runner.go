// Package runner provides a small helper for executing external commands and
// capturing their combined output. All real system state changes performed by
// tooln go through the underlying package-manager executables via this package.
package runner

import (
	"context"
	"os/exec"
	"strings"
)

// Result carries the captured output of a command run.
type Result struct {
	Output string
	Err    error
}

// Run executes name with args and returns the combined stdout+stderr output.
// A non-zero exit status is reported through the returned error while the
// output is still returned for display purposes.
func Run(ctx context.Context, name string, args ...string) Result {
	cmd := exec.CommandContext(ctx, name, args...)
	out, err := cmd.CombinedOutput()
	return Result{Output: string(out), Err: err}
}

// RunString is a convenience wrapper returning (output, error).
func RunString(ctx context.Context, name string, args ...string) (string, error) {
	r := Run(ctx, name, args...)
	return r.Output, r.Err
}

// Trim collapses surrounding whitespace from command output.
func Trim(s string) string {
	return strings.TrimSpace(s)
}
