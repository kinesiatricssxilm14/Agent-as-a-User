package main

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

// Every interaction with the system goes through this file. There is no
// simulated data anywhere in toola: the lists come from dpkg and apt, and the
// install/remove/upgrade actions are real apt-get invocations.

// aptEnv returns the environment for apt/dpkg children. Locales are forced to C
// so the parsers see stable, untranslated field names, and the frontend is set
// to noninteractive because there is no way to answer a debconf prompt from
// inside the TUI.
func aptEnv() []string {
	env := os.Environ()
	env = append(env,
		"DEBIAN_FRONTEND=noninteractive",
		"LC_ALL=C",
		"LANG=C",
		"LANGUAGE=C",
	)
	return env
}

// readTimeout bounds read-only queries so a wedged apt cannot hang the UI
// forever. Mutating operations are deliberately not bounded: killing apt-get
// midway through dpkg unpacking would leave the package database broken.
const readTimeout = 120 * time.Second

// runRead executes a read-only command and returns its stdout. stderr is
// captured separately and only surfaced when the command fails, because apt
// writes routine notices there.
func runRead(name string, args ...string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Env = aptEnv()

	var stdout, stderr strings.Builder
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()
	if ctx.Err() == context.DeadlineExceeded {
		return stdout.String(), fmt.Errorf("%s timed out after %s", name, readTimeout)
	}
	if err != nil {
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = err.Error()
		}
		return stdout.String(), fmt.Errorf("%s: %s", name, firstLine(msg))
	}
	return stdout.String(), nil
}

// runReadStreaming executes a read-only command and hands each stdout line to
// fn. apt-cache dumpavail emits ~50 MB on Debian 12; streaming keeps peak
// memory to the parsed index rather than the raw text plus the index.
func runReadStreaming(fn func(string), name string, args ...string) error {
	ctx, cancel := context.WithTimeout(context.Background(), readTimeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Env = aptEnv()

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	var stderr strings.Builder
	cmd.Stderr = &stderr

	if err := cmd.Start(); err != nil {
		return fmt.Errorf("%s: %w", name, err)
	}

	scanner := bufio.NewScanner(stdout)
	scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	for scanner.Scan() {
		fn(scanner.Text())
	}
	scanErr := scanner.Err()

	if err := cmd.Wait(); err != nil {
		if ctx.Err() == context.DeadlineExceeded {
			return fmt.Errorf("%s timed out after %s", name, readTimeout)
		}
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = err.Error()
		}
		return fmt.Errorf("%s: %s", name, firstLine(msg))
	}
	return scanErr
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return strings.TrimSpace(s[:i])
	}
	return strings.TrimSpace(s)
}

// ---------------------------------------------------------------------------
// Read-only scans
// ---------------------------------------------------------------------------

// dpkgQueryFormat asks dpkg for exactly the four fields the store needs.
// db:Status-Status distinguishes a genuinely installed package from one that was
// removed but left its configuration behind.
const dpkgQueryFormat = `${binary:Package}\t${Version}\t${db:Status-Status}\t${Architecture}\n`

// loadInstalledCmd scans the dpkg database. This is the authoritative answer to
// "what is on this system", and is re-run after every mutating operation.
func loadInstalledCmd() tea.Cmd {
	return func() tea.Msg {
		out, err := runRead("dpkg-query", "-W", "-f="+dpkgQueryFormat)
		if err != nil {
			return installedLoadedMsg{err: err}
		}
		return installedLoadedMsg{pkgs: parseDpkgQuery(out)}
	}
}

// loadAvailableCmd builds the index of installable packages by streaming
// apt-cache dumpavail through the control parser.
func loadAvailableCmd() tea.Cmd {
	return func() tea.Msg {
		// Reassemble records incrementally: feed the parser one stanza at a
		// time so the 50 MB of raw text is never resident at once.
		var (
			pkgs   []availablePkg
			seen   = make(map[string]bool)
			stanza strings.Builder
		)

		flush := func() {
			if stanza.Len() == 0 {
				return
			}
			rec := parseControl(stanza.String())
			stanza.Reset()

			name := rec.Get("Package")
			if name == "" || seen[name] {
				return
			}
			seen[name] = true

			synopsis, _ := rec.Description()
			pkgs = append(pkgs, availablePkg{
				Name:     name,
				Version:  rec.Get("Version"),
				Section:  rec.Get("Section"),
				Synopsis: synopsis,
			})
		}

		err := runReadStreaming(func(line string) {
			if strings.TrimSpace(line) == "" {
				flush()
				return
			}
			stanza.WriteString(line)
			stanza.WriteByte('\n')
		}, "apt-cache", "dumpavail")
		flush()

		if err != nil {
			return availableLoadedMsg{pkgs: pkgs, err: err}
		}
		return availableLoadedMsg{pkgs: pkgs}
	}
}

// loadUpgradableCmd asks apt what `apt-get upgrade` would do. Deriving the
// upgradable list from apt's own plan (rather than diffing installed against
// candidate versions) guarantees the property the spec requires: anything listed
// here is something apt will actually upgrade, so after upgrading it the next
// scan no longer lists it. A version diff would also surface phased or held-back
// packages that apt-get upgrade refuses to touch, which could never be cleared.
func loadUpgradableCmd() tea.Cmd {
	return func() tea.Msg {
		out, err := runRead("apt-get", "-s", "-q", "upgrade")
		if err != nil {
			return upgradableLoadedMsg{err: err}
		}
		return upgradableLoadedMsg{cands: parseInstLines(out)}
	}
}

// policyBatch bounds how many package names go into one apt-cache policy call,
// so the argument list cannot exceed the OS limit on systems with very many
// packages installed.
const policyBatch = 500

// loadPolicyCmd fills in candidate versions for installed packages. This covers
// packages that dumpavail does not describe, and gives the details pane an
// upgrade target even when apt declines to upgrade automatically.
func loadPolicyCmd(names []string) tea.Cmd {
	return func() tea.Msg {
		if len(names) == 0 {
			return policyLoadedMsg{}
		}

		var entries []policyEntry
		for start := 0; start < len(names); start += policyBatch {
			end := start + policyBatch
			if end > len(names) {
				end = len(names)
			}

			args := append([]string{"policy"}, names[start:end]...)
			out, err := runRead("apt-cache", args...)
			if err != nil {
				return policyLoadedMsg{entries: entries, err: err}
			}
			entries = append(entries, parsePolicy(out)...)
		}
		return policyLoadedMsg{entries: entries}
	}
}

// probeAptListsCmd reports whether any package index has been downloaded. A
// fresh debian:12-slim container ships with /var/lib/apt/lists empty, so without
// this check toola would show zero available packages and look broken.
func probeAptListsCmd() tea.Cmd {
	return func() tea.Msg {
		entries, err := os.ReadDir("/var/lib/apt/lists")
		if err != nil {
			return aptListsMsg{present: false}
		}
		for _, e := range entries {
			if e.IsDir() {
				continue
			}
			// Index files are named like
			// deb.debian.org_debian_dists_bookworm_main_binary-arm64_Packages.lz4
			if strings.Contains(e.Name(), "_Packages") {
				return aptListsMsg{present: true}
			}
		}
		return aptListsMsg{present: false}
	}
}

// ---------------------------------------------------------------------------
// Package details
// ---------------------------------------------------------------------------

// pkgDetails is everything the details pane shows about one package.
type pkgDetails struct {
	Name          string
	Version       string
	Section       string
	Priority      string
	Architecture  string
	Maintainer    string
	Homepage      string
	Source        string
	InstalledSize string
	Size          string
	Synopsis      string
	Long          string

	Relations []relationGroup

	// Virtual marks a package that exists only as a name provided by others;
	// Providers then lists the real packages that satisfy it.
	Virtual   bool
	Providers []string

	// Origin records where the record came from, so the pane can say whether
	// the user is looking at repository metadata or the local dpkg entry.
	Origin string
}

// DependencyCount totals every relationship entry, for the pane header.
func (d *pkgDetails) DependencyCount() int {
	n := 0
	for _, g := range d.Relations {
		n += len(g.Deps)
	}
	return n
}

// loadDetailsCmd fetches one package's full record. It tries the repository
// metadata first, then falls back to the local dpkg entry (for packages
// installed from a .deb and absent from every repo), then to the virtual-package
// provider list (for names like "awk" that no real package carries).
func loadDetailsCmd(gen int, name string) tea.Cmd {
	return func() tea.Msg {
		det, err := fetchDetails(name)
		return detailsLoadedMsg{gen: gen, name: name, details: det, err: err}
	}
}

func fetchDetails(name string) (*pkgDetails, error) {
	// apt-cache show prints one record per available version, newest first, and
	// exits 0 with no output at all for a purely virtual package.
	out, showErr := runRead("apt-cache", "show", name)
	if rec := parseControl(out); !rec.Empty() && rec.Get("Package") != "" {
		det := detailsFromRecord(rec)
		det.Origin = "apt-cache show"
		return det, nil
	}

	// Not in the repositories: ask dpkg about the installed copy.
	if local, err := runRead("dpkg-query", "-s", name); err == nil {
		if rec := parseControl(local); !rec.Empty() && rec.Get("Package") != "" {
			det := detailsFromRecord(rec)
			det.Origin = "dpkg-query -s (local package, not in any repository)"
			return det, nil
		}
	}

	// Possibly a virtual package: list what provides it.
	if showpkg, err := runRead("apt-cache", "showpkg", name); err == nil {
		if providers := parseProviders(showpkg); len(providers) > 0 {
			return &pkgDetails{
				Name:      name,
				Virtual:   true,
				Providers: providers,
				Synopsis:  "virtual package",
				Long: "This is a virtual package: no package of this name exists.\n" +
					"It is provided by the packages listed below, one of which must be\n" +
					"installed to satisfy dependencies on this name.",
				Origin: "apt-cache showpkg (virtual package)",
			}, nil
		}
	}

	if showErr != nil {
		return nil, showErr
	}
	return nil, fmt.Errorf("no information available for %q", name)
}

// detailsFromRecord projects a control record onto the details pane's fields.
func detailsFromRecord(rec *controlRecord) *pkgDetails {
	synopsis, long := rec.Description()

	return &pkgDetails{
		Name:          rec.Get("Package"),
		Version:       rec.Get("Version"),
		Section:       rec.Get("Section"),
		Priority:      rec.Get("Priority"),
		Architecture:  rec.Get("Architecture"),
		Maintainer:    rec.Get("Maintainer"),
		Homepage:      rec.Get("Homepage"),
		Source:        rec.Get("Source"),
		InstalledSize: rec.Get("Installed-Size"),
		Size:          rec.Get("Size"),
		Synopsis:      synopsis,
		Long:          long,
		Relations:     rec.relations(),
	}
}

// previewRemoveCmd asks apt which packages a removal would take with it, so the
// confirmation prompt can warn about collateral damage before anything happens.
func previewRemoveCmd(name string, purge bool) ([]string, error) {
	verb := "remove"
	if purge {
		verb = "purge"
	}
	out, err := runRead("apt-get", "-s", "-q", verb, name)
	if err != nil {
		return nil, err
	}
	return parseRemvLines(out), nil
}

// ---------------------------------------------------------------------------
// Mutating operations
// ---------------------------------------------------------------------------

// operation describes one apt-get invocation that changes system state.
type operation struct {
	Kind   opKind
	Target string // package name, empty for whole-system operations
}

type opKind int

const (
	opInstall opKind = iota
	opRemove
	opPurge
	opUpgradeOne
	opUpgradeAll
	opUpdate
)

// Verb is the human-readable name used in prompts and status messages.
func (o operation) Verb() string {
	switch o.Kind {
	case opInstall:
		return "install"
	case opRemove:
		return "remove"
	case opPurge:
		return "purge"
	case opUpgradeOne:
		return "upgrade"
	case opUpgradeAll:
		return "upgrade all packages"
	case opUpdate:
		return "update package lists"
	}
	return "operate"
}

// Describe renders the operation for the confirmation prompt.
func (o operation) Describe() string {
	if o.Target == "" {
		return o.Verb()
	}
	return fmt.Sprintf("%s %s", o.Verb(), o.Target)
}

// argv returns the exact command line for the operation. Assertions of yes are
// required because there is no way to answer an apt prompt from the TUI; note
// that no operation ever passes --force-* flags, so apt still refuses genuinely
// dangerous changes and reports why.
func (o operation) argv() []string {
	switch o.Kind {
	case opInstall:
		return []string{"apt-get", "install", "-y", "--no-install-recommends", "--", o.Target}
	case opRemove:
		return []string{"apt-get", "remove", "-y", "--", o.Target}
	case opPurge:
		return []string{"apt-get", "purge", "-y", "--", o.Target}
	case opUpgradeOne:
		return []string{"apt-get", "install", "-y", "--only-upgrade", "--", o.Target}
	case opUpgradeAll:
		return []string{"apt-get", "upgrade", "-y"}
	case opUpdate:
		return []string{"apt-get", "update"}
	}
	return nil
}

// CommandLine renders argv for display, so the log shows exactly what ran.
func (o operation) CommandLine() string {
	return strings.Join(o.argv(), " ")
}

// runner owns a single in-flight mutating operation and the channel its output
// lines arrive on.
type runner struct {
	lines chan opLineMsg
	done  chan opDoneMsg

	cancel context.CancelFunc
	mu     sync.Mutex
}

// lineBuffer is the channel depth for streamed output. apt can emit progress
// bursts faster than the UI redraws; a buffer this size means the child is never
// blocked in practice, and the drain command keeps up regardless.
const lineBuffer = 512

// start launches the operation. Output lines are pushed onto r.lines as they
// arrive so the log pane fills in live rather than after completion.
func (r *runner) start(op operation) tea.Cmd {
	r.mu.Lock()
	defer r.mu.Unlock()

	r.lines = make(chan opLineMsg, lineBuffer)
	r.done = make(chan opDoneMsg, 1)

	ctx, cancel := context.WithCancel(context.Background())
	r.cancel = cancel

	lines, done := r.lines, r.done
	argv := op.argv()

	go func() {
		defer close(lines)
		defer cancel()

		lines <- opLineMsg{line: "$ " + op.CommandLine()}

		cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
		cmd.Env = aptEnv()

		stdout, err := cmd.StdoutPipe()
		if err != nil {
			done <- opDoneMsg{op: op, err: err}
			return
		}
		stderr, err := cmd.StderrPipe()
		if err != nil {
			done <- opDoneMsg{op: op, err: err}
			return
		}

		if err := cmd.Start(); err != nil {
			done <- opDoneMsg{op: op, err: fmt.Errorf("cannot run %s: %w", argv[0], err)}
			return
		}

		// Both pipes are drained concurrently: apt interleaves progress on
		// stdout with warnings on stderr, and leaving either unread would
		// eventually block the child.
		var wg sync.WaitGroup
		pump := func(rd io.Reader, isErr bool) {
			defer wg.Done()
			scanner := bufio.NewScanner(rd)
			scanner.Buffer(make([]byte, 0, 32*1024), 1024*1024)
			for scanner.Scan() {
				text := scanner.Text()
				// dpkg draws progress with carriage returns; keep only the
				// final segment so the log does not fill with partial bars.
				if i := strings.LastIndexByte(text, '\r'); i >= 0 {
					text = text[i+1:]
				}
				if strings.TrimSpace(text) == "" {
					continue
				}
				lines <- opLineMsg{line: text, stderr: isErr}
			}
		}
		wg.Add(2)
		go pump(stdout, false)
		go pump(stderr, true)
		wg.Wait()

		err = cmd.Wait()
		if ctx.Err() == context.Canceled {
			err = errors.New("cancelled")
		}
		done <- opDoneMsg{op: op, err: err}
	}()

	return r.waitForLine()
}

// waitForLine returns a command that delivers the next output line, or the
// completion message once the stream closes. It reschedules itself, which is
// how a long-running child keeps feeding the UI without blocking it.
func (r *runner) waitForLine() tea.Cmd {
	r.mu.Lock()
	lines, done := r.lines, r.done
	r.mu.Unlock()

	if lines == nil {
		return nil
	}

	return func() tea.Msg {
		if line, ok := <-lines; ok {
			return line
		}
		// Output closed; the child has exited and posted its result.
		return <-done
	}
}

// stop cancels a running operation. It is only reachable for apt-get update,
// which is safe to interrupt; installs and removals are not cancellable because
// killing dpkg mid-unpack can leave the package database inconsistent.
func (r *runner) stop() {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.cancel != nil {
		r.cancel()
	}
}

// checkPrivileges reports a warning if toola cannot expect apt to work. It
// returns advice rather than refusing to start, because browsing and inspecting
// packages are useful without write access.
func checkPrivileges() string {
	if os.Geteuid() == 0 {
		return ""
	}
	return "not running as root: browsing works, but install/remove/upgrade will fail — start with `sudo toola`"
}

// missingTools lists any required binary that is absent from PATH, so the user
// gets one clear message instead of a failure per action.
func missingTools() []string {
	var missing []string
	for _, tool := range []string{"apt-get", "apt-cache", "dpkg-query"} {
		if _, err := exec.LookPath(tool); err != nil {
			missing = append(missing, tool)
		}
	}
	return missing
}
