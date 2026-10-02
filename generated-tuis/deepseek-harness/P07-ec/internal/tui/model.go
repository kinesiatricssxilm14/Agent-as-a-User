// Package tui implements the Bubble Tea interface for toolg.
package tui

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"toolg/internal/conflict"
	"toolg/internal/gitx"
)

// Screen identifies the currently displayed view.
type screen int

const (
	screenFiles screen = iota
	screenMerge
	screenCommit
	screenHistory
	screenHelp
)

// Panel focus targets.
const (
	sideOurs = iota
	sideResult
	sideTheirs
)

// Model is the root Bubble Tea model.
type Model struct {
	screen     screen
	prevScreen screen // where to return from the help screen

	workdir string // launch working directory (may differ from repo root)
	repo    string // absolute path of the Git repository root

	files   []string // conflicted files, relative to repo root
	curFile int      // selection index in the files list

	// Currently open file.
	path    string         // absolute path
	relPath string         // path relative to repo root (for git + display)
	cf      *conflict.File // parsed content
	current int            // index of the current conflict (0-based)
	focus   int            // focused panel: ours/result/theirs
	scroll  [3]int         // scroll offset per panel
	unsaved bool           // tracks whether there are unsaved resolutions

	// Commit screen.
	message   textinput.Model
	committed string // output of the last successful commit
	commitErr string

	// History screen.
	logLines  []string
	logScroll int

	// Feedback.
	status    string
	statusErr bool

	width, height int
	err           error
}

// New builds the model, performs repository discovery, and optionally opens a
// target file directly.
func New(workdir, targetFile string) Model {
	m := Model{
		workdir:    workdir,
		screen:     screenFiles,
		focus:      sideResult,
		prevScreen: screenFiles,
	}

	ti := textinput.New()
	ti.Placeholder = "commit message (empty = use git's merge message)"
	ti.CharLimit = 200
	ti.Width = 60
	m.message = ti

	repo, err := gitx.FindRoot(workdir)
	if err != nil {
		m.err = err
		m.setStatus("git error: "+err.Error(), true)
		return m
	}
	m.repo = repo
	m.refreshFiles()

	if targetFile != "" {
		abs := resolvePath(repo, targetFile)
		if fileExists(abs) {
			m.openFile(abs)
			return m
		}
		m.setStatus("file not found: "+targetFile, true)
	}
	if len(m.files) > 0 {
		m.screen = screenFiles
	} else {
		m.setStatus("no conflicted files found", false)
		m.screen = screenFiles
	}
	return m
}

// Init is the Bubble Tea init command.
func (m Model) Init() tea.Cmd {
	return nil
}

// resolvePath turns a user-supplied path into an absolute path relative to the
// repository root.
func resolvePath(repo, p string) string {
	if filepath.IsAbs(p) {
		return p
	}
	return filepath.Join(repo, p)
}

func fileExists(p string) bool {
	info, err := os.Stat(p)
	return err == nil && !info.IsDir()
}

// refreshFiles reloads the list of conflicted files from git.
func (m *Model) refreshFiles() {
	if m.repo == "" {
		return
	}
	files, err := gitx.ConflictedFiles(m.repo)
	if err != nil {
		m.setStatus("git status error: "+err.Error(), true)
		return
	}
	m.files = files
}

// openFile reads, parses, and opens a file in the merge view.
func (m *Model) openFile(abs string) {
	data, err := os.ReadFile(abs)
	if err != nil {
		m.setStatus("read error: "+err.Error(), true)
		return
	}
	cf, err := conflict.Parse(abs, string(data))
	if err != nil {
		m.setStatus("parse error: "+err.Error(), true)
		return
	}
	m.cf = cf
	m.path = abs
	rel, rerr := filepath.Rel(m.repo, abs)
	if rerr != nil {
		rel = abs
	}
	m.relPath = rel
	m.current = 0
	m.scroll = [3]int{}
	m.unsaved = false
	m.screen = screenMerge
	if !cf.HasConflicts() {
		m.setStatus("no conflict markers in "+rel, false)
	} else {
		m.centerOnConflict()
	}
}

func (m *Model) setStatus(msg string, isErr bool) {
	m.status = msg
	m.statusErr = isErr
}

// currentChunk returns the chunk index of the current conflict, or -1.
func (m *Model) currentChunk() int {
	if m.cf == nil {
		return -1
	}
	idx := m.cf.ConflictChunkIndices()
	if m.current < 0 || m.current >= len(idx) {
		return -1
	}
	return idx[m.current]
}

// currentResolution returns the resolution of the current conflict.
func (m *Model) currentResolution() conflict.Resolution {
	ci := m.currentChunk()
	if ci < 0 {
		return conflict.ResolveUnresolved
	}
	return m.cf.Chunks[ci].Resolution
}

// chunkSideLines returns the unstyled lines a chunk contributes to a given
// side of the three-way view.
func (m *Model) chunkSideLines(c conflict.Chunk, side int) []string {
	switch c.Kind {
	case conflict.KindPlain:
		return c.Lines
	case conflict.KindConflict:
		switch side {
		case sideOurs:
			return c.Ours
		case sideTheirs:
			return c.Theirs
		case sideResult:
			if !c.Resolved() {
				return c.RawBlock()
			}
			if c.Resolution == conflict.ResolveNone {
				return []string{"(empty — conflict discarded)"}
			}
			return c.Result
		}
	}
	return nil
}

// sideLineCounts returns, per chunk, the number of lines that chunk
// contributes to the given side.
func (m *Model) sideLineCounts(side int) []int {
	if m.cf == nil {
		return nil
	}
	counts := make([]int, len(m.cf.Chunks))
	for i, c := range m.cf.Chunks {
		counts[i] = len(m.chunkSideLines(c, side))
	}
	return counts
}

// centerOnConflict scrolls each panel so the current conflict is visible.
func (m *Model) centerOnConflict() {
	if m.cf == nil {
		return
	}
	idx := m.cf.ConflictChunkIndices()
	if len(idx) == 0 || m.current < 0 || m.current >= len(idx) {
		return
	}
	for side := 0; side < 3; side++ {
		m.scroll[side] = m.conflictStartLine(side, m.current)
	}
}

// conflictStartLine returns the rendered line index (in the given side's
// un-clipped stream) where the n-th conflict begins.
func (m *Model) conflictStartLine(side, n int) int {
	if m.cf == nil {
		return 0
	}
	counts := m.sideLineCounts(side)
	idx := m.cf.ConflictChunkIndices()
	if n < 0 || n >= len(idx) {
		return 0
	}
	line := 0
	for i := 0; i < idx[n]; i++ {
		line += counts[i]
	}
	return line
}

// nextConflict / prevConflict move the current conflict, clamping and
// re-centering the panels.
func (m *Model) nextConflict() {
	if m.cf == nil {
		return
	}
	n := m.cf.ConflictCount()
	if n == 0 {
		return
	}
	if m.current < n-1 {
		m.current++
	}
	m.centerOnConflict()
}

func (m *Model) prevConflict() {
	if m.current > 0 {
		m.current--
	}
	m.centerOnConflict()
}

// applyResolution applies a strategy to the current conflict.
func (m *Model) applyResolution(r conflict.Resolution) {
	ci := m.currentChunk()
	if ci < 0 {
		m.setStatus("no conflict selected", true)
		return
	}
	m.cf.Resolve(ci, r)
	m.unsaved = true
	m.setStatus("conflict "+itoa(m.current+1)+" resolved as: "+r.String(), false)
}

// resetResolution clears the current conflict's resolution.
func (m *Model) resetResolution() {
	ci := m.currentChunk()
	if ci < 0 {
		return
	}
	m.cf.Reset(ci)
	m.unsaved = true
	m.setStatus("conflict "+itoa(m.current+1)+" reset to unresolved", false)
}

// save writes the resolved content back to the working-tree file.
func (m *Model) save() {
	if m.cf == nil {
		m.setStatus("no file open", true)
		return
	}
	data := m.cf.Render()
	mode := os.FileMode(0o644)
	if info, err := os.Stat(m.path); err == nil {
		mode = info.Mode()
	}
	if err := os.WriteFile(m.path, []byte(data), mode); err != nil {
		m.setStatus("write error: "+err.Error(), true)
		return
	}
	m.unsaved = false
	remaining := m.cf.UnresolvedCount()
	if remaining > 0 {
		m.setStatus("saved "+m.relPath+" — "+itoa(remaining)+" conflict(s) still unresolved", false)
	} else {
		m.setStatus("saved "+m.relPath+" — all conflicts resolved", false)
	}
}

// doCommit stages resolved files and creates the commit.
func (m *Model) doCommit() {
	if m.repo == "" {
		m.setStatus("not inside a git repository", true)
		return
	}
	if err := gitx.EnsureIdentity(m.repo); err != nil {
		m.setStatus("identity error: "+err.Error(), true)
		return
	}

	// Refuse to commit while any conflicted file still carries markers.
	files, _ := gitx.ConflictedFiles(m.repo)
	var unresolved []string
	for _, f := range files {
		cf, err := conflict.Parse(f, readFileString(filepath.Join(m.repo, f)))
		if err != nil {
			continue
		}
		if cf.HasConflicts() {
			unresolved = append(unresolved, f)
		}
	}
	if len(unresolved) > 0 {
		m.setStatus("cannot commit — unresolved conflicts remain: "+strings.Join(unresolved, ", "), true)
		return
	}

	stage := m.stageList(files)
	if len(stage) == 0 {
		m.setStatus("nothing to commit", true)
		return
	}
	if _, err := gitx.Stage(m.repo, stage...); err != nil {
		m.setStatus("git add failed: "+err.Error(), true)
		return
	}

	msg := strings.TrimSpace(m.message.Value())
	noEdit := gitx.IsMerge(m.repo)
	out, err := gitx.Commit(m.repo, msg, noEdit)
	if err != nil {
		m.commitErr = err.Error()
		m.setStatus("commit failed: "+err.Error(), true)
		return
	}
	m.commitErr = ""
	m.committed = out
	m.refreshFiles()
	m.setStatus("committed: "+firstLine(out), false)
}

// stageList returns the list of paths that should be staged.
func (m *Model) stageList(conflicted []string) []string {
	var out []string
	seen := map[string]bool{}
	add := func(p string) {
		if p == "" || seen[p] {
			return
		}
		seen[p] = true
		out = append(out, p)
	}
	for _, f := range conflicted {
		add(f)
	}
	add(m.relPath)
	return out
}

// showHistory loads git log into the history screen.
func (m *Model) showHistory() {
	if m.repo == "" {
		m.setStatus("not inside a git repository", true)
		return
	}
	out, err := gitx.Log(m.repo, 100)
	if err != nil {
		m.setStatus("git log failed: "+err.Error(), true)
		return
	}
	if out == "" {
		m.logLines = []string{"(no commits yet)"}
	} else {
		m.logLines = strings.Split(out, "\n")
	}
	m.logScroll = 0
	m.prevScreen = m.screen
	m.screen = screenHistory
}

// showHelp opens the help screen, remembering where to return.
func (m *Model) showHelp() {
	m.prevScreen = m.screen
	m.screen = screenHelp
}

func readFileString(p string) string {
	b, err := os.ReadFile(p)
	if err != nil {
		return ""
	}
	return string(b)
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i]
	}
	return s
}

// itoa is a tiny integer formatter to avoid importing strconv repeatedly.
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
