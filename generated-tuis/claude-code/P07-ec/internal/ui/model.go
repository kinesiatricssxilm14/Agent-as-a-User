package ui

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"github.com/toolg/toolg/internal/conflict"
	"github.com/toolg/toolg/internal/gitx"
)

// view is the top-level screen being displayed. Each view fills the window on
// its own; no view hides information belonging to another, and the merge view
// always shows all three panels together.
type view int

const (
	viewMerge view = iota
	viewFiles
	viewHistory
	viewHelp
)

// modal is an input overlay that needs the keyboard. Only prompts that
// genuinely require typed input use one, and each is a small bar rather than a
// panel that covers the merge content.
type modal int

const (
	modalNone modal = iota
	modalCommit
	modalConfirmAbort
	modalEditBlock
)

// statusKind selects the colour of the status message.
type statusKind int

const (
	statusInfo statusKind = iota
	statusSuccess
	statusWarning
	statusError
)

// Config holds the settings resolved from the command line.
type Config struct {
	// WorkDir is the git working directory; git commands run here.
	WorkDir string
	// File is the conflict file to open, relative to WorkDir or absolute.
	File string
	// LogLimit caps how many commits the history view loads.
	LogLimit int
}

// Model is the Bubble Tea model for the whole application.
type Model struct {
	cfg   Config
	keys  keyMap
	theme *theme

	repo *gitx.Repo

	// file is the parsed conflict file, nil if it could not be loaded.
	file     *conflict.File
	fileMode os.FileMode
	// absPath is the resolved path of the open file, relPath its
	// repository-relative form for git commands.
	absPath string
	relPath string

	// lay is the current row layout, rebuilt whenever a resolution changes.
	lay *layout

	status *gitx.Status
	log    []gitx.LogEntry
	// conflictFiles lists other unmerged paths, so the user can switch files.
	conflictFiles []string

	view     view
	modal    modal
	focus    side
	showBase bool
	// dirty records unsaved resolution changes.
	dirty bool
	// savedOnce records that a write-back has happened, which gates the hint
	// to commit.
	savedOnce bool

	// cursor is the selected row in the merge view.
	cursor int
	// offset is the first visible row, and hoff the horizontal scroll amount.
	offset int
	hoff   int

	// filesCursor and logCursor track selection in the list views.
	filesCursor int
	logCursor   int
	logOffset   int
	helpOffset  int

	commitInput textinput.Model
	blockEdit   textarea.Model
	// editingBlock is the block being hand-edited via modalEditBlock.
	editingBlock int

	statusMsg  string
	statusKind statusKind
	// gitOutput holds the last git command's output for display.
	gitOutput string

	width  int
	height int

	// loadErr records why the file could not be opened, shown in place of the
	// panels.
	loadErr error
}

// New builds the initial model and performs the first real reads of the
// repository and the conflict file.
func New(cfg Config) (*Model, error) {
	if cfg.LogLimit <= 0 {
		cfg.LogLimit = 200
	}
	if err := gitx.Available(); err != nil {
		return nil, err
	}
	repo, err := gitx.Open(cfg.WorkDir)
	if err != nil {
		return nil, err
	}

	ci := textinput.New()
	ci.Placeholder = "Merge commit message"
	ci.CharLimit = 0
	ci.Prompt = ""

	ta := textarea.New()
	ta.Placeholder = "Resolved content for this conflict block"
	ta.ShowLineNumbers = true
	ta.CharLimit = 0

	m := &Model{
		cfg:         cfg,
		keys:        newKeyMap(),
		theme:       newTheme(),
		repo:        repo,
		view:        viewMerge,
		focus:       sideResult,
		commitInput: ci,
		blockEdit:   ta,
		width:       80,
		height:      24,
	}

	m.absPath = cfg.File
	if !filepath.IsAbs(m.absPath) {
		m.absPath = filepath.Join(repo.Dir, cfg.File)
	}
	if rel, err := repo.RelPath(cfg.File); err == nil {
		m.relPath = rel
	} else {
		m.relPath = filepath.Base(m.absPath)
	}

	m.refreshStatus()
	m.loadFile()
	return m, nil
}

// Init satisfies tea.Model. History and the conflicted-file list are loaded up
// front so switching views never shows a blank screen.
func (m *Model) Init() tea.Cmd {
	return tea.Batch(tea.EnterAltScreen, m.loadLogCmd(), tea.WindowSize())
}

// --- data loading -----------------------------------------------------------

// loadFile reads and parses the conflict file from disk.
func (m *Model) loadFile() {
	f, mode, err := conflict.Load(m.absPath)
	if err != nil {
		m.loadErr = err
		m.file = nil
		m.lay = nil
		return
	}
	m.loadErr = nil
	m.file = f
	m.fileMode = mode
	m.dirty = false
	m.rebuild()
	m.cursor = 0
	m.offset = 0
	if len(f.Blocks) > 0 {
		m.jumpToBlock(0)
	}
}

// rebuild regenerates the row layout after any change to resolutions.
func (m *Model) rebuild() {
	if m.file == nil {
		m.lay = nil
		return
	}
	m.lay = buildLayout(m.file, m.showBase)
	if m.cursor >= len(m.lay.rows) {
		m.cursor = maxInt(0, len(m.lay.rows)-1)
	}
}

// refreshStatus re-reads repository state and the conflicted file list.
func (m *Model) refreshStatus() {
	st, err := m.repo.Status()
	if err != nil {
		m.setStatus(statusError, err.Error())
		return
	}
	m.status = st
	if files, err := m.repo.ConflictedFiles(); err == nil {
		m.conflictFiles = files
	}
}

// logLoadedMsg carries history loaded off the UI goroutine.
type logLoadedMsg struct {
	entries []gitx.LogEntry
	err     error
}

func (m *Model) loadLogCmd() tea.Cmd {
	repo, limit := m.repo, m.cfg.LogLimit
	return func() tea.Msg {
		entries, err := repo.Log(limit)
		return logLoadedMsg{entries: entries, err: err}
	}
}

// gitDoneMsg reports the outcome of a mutating git command.
type gitDoneMsg struct {
	action string
	output string
	err    error
}

func (m *Model) commitCmd(message string) tea.Cmd {
	repo, rel := m.repo, m.relPath
	return func() tea.Msg {
		// Staging the resolved file is what clears git's unmerged entry; a
		// merge commit is refused while any remain.
		if err := repo.Stage(rel); err != nil {
			return gitDoneMsg{action: "stage", err: err}
		}
		out, err := repo.Commit(message)
		return gitDoneMsg{action: "commit", output: out, err: err}
	}
}

func (m *Model) abortCmd() tea.Cmd {
	repo := m.repo
	return func() tea.Msg {
		out, err := repo.AbortMerge()
		return gitDoneMsg{action: "abort", output: out, err: err}
	}
}

// --- status messages --------------------------------------------------------

func (m *Model) setStatus(kind statusKind, format string, args ...any) {
	m.statusKind = kind
	m.statusMsg = fmt.Sprintf(format, args...)
}

// --- Bubble Tea plumbing ----------------------------------------------------

// Update handles all messages. Modal input is dispatched first so that typing a
// commit message cannot be intercepted by the single-letter action keys.
func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.commitInput.Width = maxInt(20, m.width-24)
		m.blockEdit.SetWidth(maxInt(20, m.width-8))
		m.blockEdit.SetHeight(clampInt(m.height/3, 3, 12))
		m.ensureVisible()
		return m, nil

	case logLoadedMsg:
		if msg.err != nil {
			m.setStatus(statusError, "git log: %v", msg.err)
			return m, nil
		}
		m.log = msg.entries
		return m, nil

	case gitDoneMsg:
		return m, m.handleGitDone(msg)

	case tea.KeyMsg:
		if m.modal != modalNone {
			return m.updateModal(msg)
		}
		return m.updateKey(msg)
	}
	return m, nil
}

func (m *Model) handleGitDone(msg gitDoneMsg) tea.Cmd {
	m.gitOutput = msg.output
	if msg.err != nil {
		m.setStatus(statusError, "%s failed: %v", msg.action, msg.err)
		m.refreshStatus()
		return nil
	}
	switch msg.action {
	case "commit":
		m.setStatus(statusSuccess, "merge committed")
		m.refreshStatus()
		// The file is no longer in conflict, so reload it and show the new
		// history, which is the natural next thing to look at.
		m.loadFile()
		m.view = viewHistory
		m.logCursor = 0
		m.logOffset = 0
		return m.loadLogCmd()
	case "abort":
		m.setStatus(statusWarning, "merge aborted; working tree restored")
		m.refreshStatus()
		m.loadFile()
		return m.loadLogCmd()
	}
	m.refreshStatus()
	return nil
}

// updateModal routes keys to whichever input overlay is active.
func (m *Model) updateModal(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch m.modal {
	case modalCommit:
		switch {
		case key.Matches(msg, m.keys.Cancel):
			m.modal = modalNone
			m.setStatus(statusInfo, "commit cancelled")
			return m, nil
		case key.Matches(msg, m.keys.Confirm):
			text := strings.TrimSpace(m.commitInput.Value())
			if text == "" {
				m.setStatus(statusError, "commit message must not be empty")
				return m, nil
			}
			m.modal = modalNone
			m.setStatus(statusInfo, "committing…")
			return m, m.commitCmd(text)
		}
		var cmd tea.Cmd
		m.commitInput, cmd = m.commitInput.Update(msg)
		return m, cmd

	case modalConfirmAbort:
		switch {
		case key.Matches(msg, m.keys.Cancel), msg.String() == "n", msg.String() == "N":
			m.modal = modalNone
			m.setStatus(statusInfo, "abort cancelled")
			return m, nil
		case msg.String() == "y", msg.String() == "Y", key.Matches(msg, m.keys.Confirm):
			m.modal = modalNone
			m.setStatus(statusInfo, "aborting merge…")
			return m, m.abortCmd()
		}
		return m, nil

	case modalEditBlock:
		// Esc leaves the editor; ctrl+s applies. Enter must stay available for
		// newlines inside the text area.
		switch {
		case key.Matches(msg, m.keys.Cancel):
			m.modal = modalNone
			m.blockEdit.Blur()
			m.setStatus(statusInfo, "edit cancelled")
			return m, nil
		case msg.String() == "ctrl+s":
			m.applyBlockEdit()
			return m, nil
		}
		var cmd tea.Cmd
		m.blockEdit, cmd = m.blockEdit.Update(msg)
		return m, cmd
	}
	return m, nil
}

// applyBlockEdit stores hand-edited content as the block's resolution.
func (m *Model) applyBlockEdit() {
	if m.file == nil || m.editingBlock < 0 || m.editingBlock >= len(m.file.Blocks) {
		m.modal = modalNone
		return
	}
	b := m.file.Blocks[m.editingBlock]
	text := m.blockEdit.Value()
	// An empty editor means "no lines", which is different from one blank line.
	if text == "" {
		b.Custom = nil
	} else {
		b.Custom = strings.Split(text, "\n")
	}
	b.Choice = conflict.ChoiceCustom
	m.modal = modalNone
	m.blockEdit.Blur()
	m.dirty = true
	m.rebuild()
	m.setStatus(statusSuccess, "conflict %d set from editor", m.editingBlock+1)
}

// updateKey handles keys when no modal is active.
func (m *Model) updateKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	// Quit is global. 'q' would be ambiguous while typing, but no modal is
	// active here.
	if key.Matches(msg, m.keys.Quit) {
		return m, tea.Quit
	}

	if key.Matches(msg, m.keys.Help) {
		if m.view == viewHelp {
			m.view = viewMerge
		} else {
			m.view = viewHelp
			m.helpOffset = 0
		}
		return m, nil
	}

	// View switches are available from anywhere.
	switch {
	case key.Matches(msg, m.keys.Merge):
		m.view = viewMerge
		return m, nil
	case key.Matches(msg, m.keys.History):
		m.view = viewHistory
		return m, m.loadLogCmd()
	case key.Matches(msg, m.keys.Files):
		m.refreshStatus()
		m.view = viewFiles
		return m, nil
	}

	switch m.view {
	case viewMerge:
		return m.updateMergeKey(msg)
	case viewHistory:
		return m.updateHistoryKey(msg)
	case viewFiles:
		return m.updateFilesKey(msg)
	case viewHelp:
		return m.updateHelpKey(msg)
	}
	return m, nil
}

func (m *Model) updateHelpKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case key.Matches(msg, m.keys.Cancel):
		m.view = viewMerge
	case key.Matches(msg, m.keys.Down):
		m.helpOffset++
	case key.Matches(msg, m.keys.Up):
		m.helpOffset = maxInt(0, m.helpOffset-1)
	case key.Matches(msg, m.keys.Home):
		m.helpOffset = 0
	}
	return m, nil
}

func (m *Model) updateHistoryKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	page := maxInt(1, m.listViewportHeight())
	switch {
	case key.Matches(msg, m.keys.Cancel):
		m.view = viewMerge
	case key.Matches(msg, m.keys.Down):
		m.logCursor = minInt(m.logCursor+1, maxInt(0, len(m.log)-1))
	case key.Matches(msg, m.keys.Up):
		m.logCursor = maxInt(0, m.logCursor-1)
	case key.Matches(msg, m.keys.PageDown):
		m.logCursor = minInt(m.logCursor+page, maxInt(0, len(m.log)-1))
	case key.Matches(msg, m.keys.PageUp):
		m.logCursor = maxInt(0, m.logCursor-page)
	case key.Matches(msg, m.keys.Home):
		m.logCursor = 0
	case key.Matches(msg, m.keys.End):
		m.logCursor = maxInt(0, len(m.log)-1)
	case key.Matches(msg, m.keys.Reload):
		return m, m.loadLogCmd()
	}
	// Keep the selection inside the visible window.
	if m.logCursor < m.logOffset {
		m.logOffset = m.logCursor
	}
	if m.logCursor >= m.logOffset+page {
		m.logOffset = m.logCursor - page + 1
	}
	return m, nil
}

func (m *Model) updateFilesKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case key.Matches(msg, m.keys.Cancel):
		m.view = viewMerge
	case key.Matches(msg, m.keys.Down):
		m.filesCursor = minInt(m.filesCursor+1, maxInt(0, len(m.conflictFiles)-1))
	case key.Matches(msg, m.keys.Up):
		m.filesCursor = maxInt(0, m.filesCursor-1)
	case key.Matches(msg, m.keys.Home):
		m.filesCursor = 0
	case key.Matches(msg, m.keys.End):
		m.filesCursor = maxInt(0, len(m.conflictFiles)-1)
	case key.Matches(msg, m.keys.Reload):
		m.refreshStatus()
	case key.Matches(msg, m.keys.Confirm):
		if m.filesCursor < len(m.conflictFiles) {
			return m, m.openFile(m.conflictFiles[m.filesCursor])
		}
	}
	return m, nil
}

// openFile switches the merge view to another conflicted path. Unsaved work is
// refused rather than silently dropped.
func (m *Model) openFile(repoRelPath string) tea.Cmd {
	if m.dirty {
		m.setStatus(statusWarning, "unsaved changes in %s — press s to save or r to discard", m.relPath)
		return nil
	}
	root, err := m.repo.Run("rev-parse", "--show-toplevel")
	if err != nil {
		m.setStatus(statusError, "%v", err)
		return nil
	}
	m.absPath = filepath.Join(root, repoRelPath)
	m.relPath = repoRelPath
	m.loadFile()
	m.view = viewMerge
	if m.loadErr != nil {
		m.setStatus(statusError, "%v", m.loadErr)
	} else {
		m.setStatus(statusSuccess, "opened %s", repoRelPath)
	}
	return nil
}

// updateMergeKey handles the three-way merge view.
func (m *Model) updateMergeKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	page := maxInt(1, m.mergeViewportHeight())

	switch {
	// --- navigation ---
	case key.Matches(msg, m.keys.Down):
		m.moveCursor(1)
	case key.Matches(msg, m.keys.Up):
		m.moveCursor(-1)
	case key.Matches(msg, m.keys.PageDown):
		m.moveCursor(page)
	case key.Matches(msg, m.keys.PageUp):
		m.moveCursor(-page)
	case key.Matches(msg, m.keys.Home):
		m.cursor = 0
		m.ensureVisible()
	case key.Matches(msg, m.keys.End):
		if m.lay != nil {
			m.cursor = maxInt(0, len(m.lay.rows)-1)
		}
		m.ensureVisible()
	case key.Matches(msg, m.keys.Right):
		m.hoff += 8
	case key.Matches(msg, m.keys.Left):
		m.hoff = maxInt(0, m.hoff-8)

	// --- conflict traversal ---
	case key.Matches(msg, m.keys.NextConflict):
		m.gotoConflict(1)
	case key.Matches(msg, m.keys.PrevConflict):
		m.gotoConflict(-1)

	// --- panel focus ---
	case key.Matches(msg, m.keys.NextPanel):
		m.focus = m.cyclePanel(1)
	case key.Matches(msg, m.keys.PrevPanel):
		m.focus = m.cyclePanel(-1)
	case key.Matches(msg, m.keys.ToggleBase):
		m.showBase = !m.showBase
		if !m.showBase && m.focus == sideBase {
			m.focus = sideResult
		}
		m.rebuild()
		if m.showBase {
			m.setStatus(statusInfo, "base (common ancestor) panel shown")
		} else {
			m.setStatus(statusInfo, "base panel hidden")
		}

	// --- resolution ---
	case key.Matches(msg, m.keys.Ours):
		m.resolveCurrent(conflict.ChoiceOurs)
	case key.Matches(msg, m.keys.Theirs):
		m.resolveCurrent(conflict.ChoiceTheirs)
	case key.Matches(msg, m.keys.Both):
		m.resolveCurrent(conflict.ChoiceBoth)
	case key.Matches(msg, m.keys.None):
		m.resolveCurrent(conflict.ChoiceNone)
	case key.Matches(msg, m.keys.Clear):
		m.resolveCurrent(conflict.ChoiceUnresolved)
	case key.Matches(msg, m.keys.AllOurs):
		m.resolveAll(conflict.ChoiceOurs)
	case key.Matches(msg, m.keys.AllTheirs):
		m.resolveAll(conflict.ChoiceTheirs)
	case key.Matches(msg, m.keys.AllBoth):
		m.resolveAll(conflict.ChoiceBoth)
	case key.Matches(msg, m.keys.Edit):
		m.startBlockEdit()

	// --- git actions ---
	case key.Matches(msg, m.keys.Save):
		m.save()
	case key.Matches(msg, m.keys.Reload):
		m.loadFile()
		m.refreshStatus()
		if m.loadErr != nil {
			m.setStatus(statusError, "%v", m.loadErr)
		} else {
			m.setStatus(statusInfo, "reloaded %s from disk", m.relPath)
		}
	case key.Matches(msg, m.keys.Commit):
		return m, m.startCommit()
	case key.Matches(msg, m.keys.Abort):
		if m.status != nil && !m.status.Merging {
			m.setStatus(statusWarning, "no merge in progress to abort")
			return m, nil
		}
		m.modal = modalConfirmAbort
	}
	return m, nil
}

// cyclePanel moves focus between the visible panels only, so hiding the base
// panel does not leave focus stranded on it.
func (m *Model) cyclePanel(delta int) side {
	order := m.visibleSides()
	idx := 0
	for i, s := range order {
		if s == m.focus {
			idx = i
			break
		}
	}
	idx = (idx + delta + len(order)) % len(order)
	return order[idx]
}

// visibleSides lists the panels currently on screen, left to right.
func (m *Model) visibleSides() []side {
	if m.showBase {
		return []side{sideOurs, sideBase, sideResult, sideTheirs}
	}
	return []side{sideOurs, sideResult, sideTheirs}
}

func (m *Model) moveCursor(delta int) {
	if m.lay == nil || len(m.lay.rows) == 0 {
		return
	}
	m.cursor = clampInt(m.cursor+delta, 0, len(m.lay.rows)-1)
	m.ensureVisible()
}

// currentBlock returns the conflict block under the cursor, or nil when the
// cursor sits on a context line.
func (m *Model) currentBlock() *conflict.Block {
	if m.file == nil || m.lay == nil || m.cursor >= len(m.lay.rows) {
		return nil
	}
	idx := m.lay.rows[m.cursor].blockIndex
	if idx < 0 || idx >= len(m.file.Blocks) {
		return nil
	}
	return m.file.Blocks[idx]
}

// gotoConflict jumps to the next or previous conflict block relative to the
// cursor, wrapping at the ends so repeated presses cycle through all of them.
func (m *Model) gotoConflict(delta int) {
	if m.file == nil || len(m.file.Blocks) == 0 {
		m.setStatus(statusInfo, "no conflicts in this file")
		return
	}
	cur := -1
	if m.lay != nil && m.cursor < len(m.lay.rows) {
		cur = m.lay.rows[m.cursor].blockIndex
	}

	var target int
	switch {
	case cur >= 0:
		target = (cur + delta + len(m.file.Blocks)) % len(m.file.Blocks)
	case delta > 0:
		// From a context line, move to the first block that starts below the
		// cursor.
		target = 0
		for i := range m.file.Blocks {
			if m.lay.blockRow[i] > m.cursor {
				target = i
				break
			}
		}
	default:
		target = len(m.file.Blocks) - 1
		for i := len(m.file.Blocks) - 1; i >= 0; i-- {
			if m.lay.blockRow[i] < m.cursor {
				target = i
				break
			}
		}
	}
	m.jumpToBlock(target)
	b := m.file.Blocks[target]
	m.setStatus(statusInfo, "conflict %d/%d (line %d, %s)",
		target+1, len(m.file.Blocks), b.StartLine, b.Choice)
}

// jumpToBlock places the cursor on a block's header row.
func (m *Model) jumpToBlock(idx int) {
	if m.lay == nil || idx < 0 || idx >= len(m.lay.blockRow) {
		return
	}
	m.cursor = m.lay.blockRow[idx]
	m.ensureVisible()
}

// resolveCurrent applies a strategy to the block under the cursor.
func (m *Model) resolveCurrent(c conflict.Choice) {
	b := m.currentBlock()
	if b == nil {
		m.setStatus(statusWarning, "cursor is not inside a conflict — press n to jump to one")
		return
	}
	b.Choice = c
	if c != conflict.ChoiceCustom {
		b.Custom = nil
	}
	m.dirty = true
	// Rebuilding can change row count, so anchor the cursor to the same block.
	idx := b.Index
	m.rebuild()
	m.jumpToBlock(idx)

	if c == conflict.ChoiceUnresolved {
		m.setStatus(statusInfo, "conflict %d reset to unresolved", idx+1)
	} else {
		m.setStatus(statusSuccess, "conflict %d → %s  (%d of %d left)",
			idx+1, c, m.file.UnresolvedCount(), len(m.file.Blocks))
	}
}

func (m *Model) resolveAll(c conflict.Choice) {
	if m.file == nil || len(m.file.Blocks) == 0 {
		m.setStatus(statusInfo, "no conflicts in this file")
		return
	}
	m.file.ResolveAll(c)
	m.dirty = true
	m.rebuild()
	m.setStatus(statusSuccess, "all %d conflicts → %s", len(m.file.Blocks), c)
}

// startBlockEdit opens the text area seeded with the block's current result, so
// hand editing starts from whatever is already chosen.
func (m *Model) startBlockEdit() {
	b := m.currentBlock()
	if b == nil {
		m.setStatus(statusWarning, "cursor is not inside a conflict — press n to jump to one")
		return
	}
	seed := b.Result()
	if !b.Choice.Resolved() {
		// Unresolved blocks would seed the editor with marker lines, which is
		// never what the user wants; offer both sides instead.
		seed = append(append([]string{}, b.Ours...), b.Theirs...)
	}
	m.blockEdit.SetValue(strings.Join(seed, "\n"))
	m.blockEdit.SetHeight(clampInt(m.height/3, 3, 12))
	m.blockEdit.SetWidth(maxInt(20, m.width-8))
	m.blockEdit.Focus()
	m.blockEdit.CursorEnd()
	m.editingBlock = b.Index
	m.modal = modalEditBlock
}

// save writes the resolved file back to the working tree.
func (m *Model) save() {
	if m.file == nil {
		m.setStatus(statusError, "nothing to save: %v", m.loadErr)
		return
	}
	if err := m.file.Save(m.absPath, m.fileMode); err != nil {
		m.setStatus(statusError, "save failed: %v", err)
		return
	}
	m.dirty = false
	m.savedOnce = true
	left := m.file.UnresolvedCount()

	if left > 0 {
		m.refreshStatus()
		m.setStatus(statusWarning,
			"saved %s — %d conflict(s) still unresolved, markers kept", m.relPath, left)
		return
	}

	// A fully resolved file is staged immediately, because in git `git add` is
	// what marks a conflict resolved. Without it the path stays unmerged and a
	// later commit is refused -- which matters most in a multi-file merge, where
	// the user moves on to the next file and would otherwise leave this one
	// behind.
	if m.status != nil && m.status.Merging {
		if err := m.repo.Stage(m.relPath); err != nil {
			m.refreshStatus()
			m.setStatus(statusError, "saved %s but staging failed: %v", m.relPath, err)
			return
		}
	}
	m.refreshStatus()

	if n := len(m.conflictFiles); n > 0 {
		m.setStatus(statusSuccess,
			"saved and staged %s — %d conflicted file(s) left, press f to pick the next",
			m.relPath, n)
		return
	}
	m.setStatus(statusSuccess, "saved and staged %s — all conflicts resolved, press c to commit", m.relPath)
}

// otherConflicts lists unmerged paths other than the file currently open, so
// the commit path can explain exactly what is still outstanding.
func (m *Model) otherConflicts() []string {
	var out []string
	for _, p := range m.conflictFiles {
		if p != m.relPath {
			out = append(out, p)
		}
	}
	return out
}

// truncateList shortens a list for display, marking how many were elided.
func truncateList(items []string, n int) []string {
	if len(items) <= n {
		return items
	}
	out := append([]string{}, items[:n]...)
	return append(out, fmt.Sprintf("+%d more", len(items)-n))
}

// startCommit validates that committing is possible, then opens the message
// prompt seeded with git's own prepared merge message.
func (m *Model) startCommit() tea.Cmd {
	if m.file != nil && m.file.UnresolvedCount() > 0 {
		m.setStatus(statusError,
			"cannot commit: %d conflict(s) unresolved in %s", m.file.UnresolvedCount(), m.relPath)
		return nil
	}
	if m.dirty {
		// Committing unsaved work would commit stale bytes; save first so the
		// commit matches what is on screen.
		m.save()
		if m.dirty {
			return nil
		}
	}
	if m.file != nil && !m.savedOnce && len(m.file.Blocks) > 0 {
		m.save()
	}

	m.refreshStatus()
	if m.status != nil && !m.status.Merging {
		// Outside a merge there is nothing to conclude; committing anyway
		// would create an unrelated commit.
		if len(m.status.Conflicted) == 0 && len(m.status.Staged) == 0 && len(m.status.Modified) == 0 {
			m.setStatus(statusWarning, "no merge in progress and nothing to commit")
			return nil
		}
	}

	// Other paths may still be unmerged even though this file is done. Saying
	// so up front, and naming them, is more useful than letting git fail with a
	// generic message.
	if others := m.otherConflicts(); len(others) > 0 {
		m.setStatus(statusError,
			"cannot commit: %d other file(s) still conflicted (%s) — press f to open them",
			len(others), strings.Join(truncateList(others, 3), ", "))
		return nil
	}

	msg := m.repo.DefaultMergeMessage()
	if msg == "" {
		branch := "branch"
		if m.status != nil && m.status.MergeHead != "" {
			branch = m.status.MergeHead
		}
		msg = fmt.Sprintf("Merge branch '%s'", branch)
	}
	m.commitInput.SetValue(msg)
	m.commitInput.CursorEnd()
	m.commitInput.Focus()
	m.modal = modalCommit
	return textinput.Blink
}

// --- geometry helpers -------------------------------------------------------

// chromeHeight is the number of rows consumed by everything outside the panel
// content: header, panel frame, footer, and any modal or git output.
func (m *Model) chromeHeight() int {
	// header (2) + panel top border, title, rule, bottom border (4) + footer (2)
	h := 2 + 4 + 2
	if m.modal != modalNone {
		h += m.modalHeight()
	}
	if m.gitOutput != "" {
		h += minInt(3, len(strings.Split(strings.TrimSpace(m.gitOutput), "\n")))
	}
	return h
}

func (m *Model) modalHeight() int {
	switch m.modal {
	case modalCommit:
		// two content lines plus the box border
		return 4
	case modalConfirmAbort:
		return 4
	case modalEditBlock:
		return m.blockEdit.Height() + 4
	}
	return 0
}

// mergeViewportHeight is how many content rows the merge panels can show.
func (m *Model) mergeViewportHeight() int {
	return maxInt(1, m.height-m.chromeHeight())
}

// listViewportHeight is the row budget inside the list views' panel frame,
// which uses the same chrome as the merge view minus the panel's own title
// rows, since those are drawn as part of the list content.
func (m *Model) listViewportHeight() int {
	h := 2 + 2 + 2 // header + panel borders + footer
	if m.gitOutput != "" {
		h += minInt(3, len(strings.Split(strings.TrimSpace(m.gitOutput), "\n")))
	}
	return maxInt(1, m.height-h)
}

// ensureVisible scrolls the merge viewport so the cursor stays on screen, with
// a small margin so context above and below remains visible.
func (m *Model) ensureVisible() {
	if m.lay == nil {
		return
	}
	h := m.mergeViewportHeight()
	const margin = 2

	if m.cursor < m.offset+margin {
		m.offset = maxInt(0, m.cursor-margin)
	}
	if m.cursor >= m.offset+h-margin {
		m.offset = m.cursor - h + 1 + margin
	}
	maxOff := maxInt(0, len(m.lay.rows)-h)
	m.offset = clampInt(m.offset, 0, maxOff)
}

// --- small numeric helpers --------------------------------------------------

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func clampInt(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}
