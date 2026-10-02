package ui

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"

	"github.com/toolg/toolg/internal/conflict"
	"github.com/toolg/toolg/internal/gitx"
)

func init() {
	// Force colour output so background-highlight assertions see real escape
	// sequences even when tests run without a TTY.
	lipgloss.SetColorProfile(termenv.TrueColor)
}

// newRepo builds a real repository stopped on a conflict in conflict.py. The
// conflicting hunk is multi-line and the two sides differ in length, which is
// what exercises panel alignment.
func newRepo(t *testing.T) string {
	t.Helper()
	if err := gitx.Available(); err != nil {
		t.Skip("git not available")
	}
	dir := t.TempDir()

	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=T", "GIT_AUTHOR_EMAIL=anonymous@example.invalid",
			"GIT_COMMITTER_NAME=T", "GIT_COMMITTER_EMAIL=anonymous@example.invalid",
			"GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_SYSTEM=/dev/null", "LC_ALL=C",
		)
		out, err := cmd.CombinedOutput()
		if err != nil && args[0] != "merge" {
			t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
		}
	}
	write := func(name, content string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	run("init", "-q", "-b", "main")
	run("config", "user.name", "T")
	run("config", "user.email", "anonymous@example.invalid")
	run("config", "commit.gpgsign", "false")

	write("conflict.py", "import sys\n\ndef main():\n    value = 0\n    print(value)\n\nmain()\n")
	run("add", ".")
	run("commit", "-q", "-m", "initial commit")

	run("checkout", "-q", "-b", "feature")
	write("conflict.py", "import sys\n\ndef main():\n    value = feature_value()\n    log(value)\n    print(value)\n\nmain()\n")
	run("commit", "-q", "-am", "feature change")

	run("checkout", "-q", "main")
	write("conflict.py", "import sys\n\ndef main():\n    value = main_value()\n    print(value)\n\nmain()\n")
	run("commit", "-q", "-am", "main change")

	run("merge", "feature")
	return dir
}

func newModel(t *testing.T) (*Model, string) {
	t.Helper()
	dir := newRepo(t)
	m, err := New(Config{WorkDir: dir, File: "conflict.py", LogLimit: 50})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	// A realistic window so panel widths and the viewport are exercised.
	m.Update(tea.WindowSizeMsg{Width: 160, Height: 40})
	return m, dir
}

// press feeds a keystroke through Update the way Bubble Tea would, and runs any
// returned command synchronously so git results are applied.
func press(t *testing.T, m *Model, keys ...string) {
	t.Helper()
	for _, k := range keys {
		var msg tea.KeyMsg
		switch k {
		case "enter":
			msg = tea.KeyMsg{Type: tea.KeyEnter}
		case "esc":
			msg = tea.KeyMsg{Type: tea.KeyEscape}
		case "tab":
			msg = tea.KeyMsg{Type: tea.KeyTab}
		case "up":
			msg = tea.KeyMsg{Type: tea.KeyUp}
		case "down":
			msg = tea.KeyMsg{Type: tea.KeyDown}
		case "ctrl+s":
			msg = tea.KeyMsg{Type: tea.KeyCtrlS}
		default:
			msg = tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(k)}
		}
		_, cmd := m.Update(msg)
		drain(t, m, cmd)
	}
}

// drain executes a command tree and feeds the resulting messages back in, which
// is how the real event loop applies git output.
func drain(t *testing.T, m *Model, cmd tea.Cmd) {
	t.Helper()
	for i := 0; cmd != nil && i < 16; i++ {
		msg := cmd()
		switch msg.(type) {
		case nil:
			return
		case tea.QuitMsg:
			return
		}
		// Batch messages need each child running; only the shapes this app
		// produces are handled.
		if batch, ok := msg.(tea.BatchMsg); ok {
			var next []tea.Cmd
			for _, c := range batch {
				if c != nil {
					next = append(next, c)
				}
			}
			for _, c := range next {
				drain(t, m, c)
			}
			return
		}
		_, cmd = m.Update(msg)
	}
}

func readFile(t *testing.T, dir, name string) string {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

// --- construction -----------------------------------------------------------

func TestNewRejectsNonRepo(t *testing.T) {
	if _, err := New(Config{WorkDir: t.TempDir(), File: "x"}); err == nil {
		t.Error("want error outside a git repository")
	}
}

func TestNewParsesConflict(t *testing.T) {
	m, _ := newModel(t)
	if m.loadErr != nil {
		t.Fatalf("loadErr = %v", m.loadErr)
	}
	if m.file == nil || len(m.file.Blocks) != 1 {
		t.Fatalf("want 1 conflict block, got %v", m.file)
	}
	if m.relPath != "conflict.py" {
		t.Errorf("relPath = %q", m.relPath)
	}
	if m.status == nil || !m.status.Merging {
		t.Error("model did not detect the in-progress merge")
	}
	// The cursor should start on the first conflict rather than at line 1, so
	// the user lands on the work.
	if got := m.lay.rows[m.cursor].blockIndex; got != 0 {
		t.Errorf("cursor starts on block %d, want 0", got)
	}
}

func TestMissingFileIsReported(t *testing.T) {
	dir := newRepo(t)
	m, err := New(Config{WorkDir: dir, File: "nope.py"})
	if err != nil {
		t.Fatalf("New should succeed and report the error in-UI: %v", err)
	}
	if m.loadErr == nil {
		t.Error("loadErr = nil for a missing file")
	}
	// The view must explain the problem instead of rendering empty panels.
	if out := m.View(); !strings.Contains(out, "Cannot open") {
		t.Errorf("view does not report the failure:\n%s", out)
	}
}

// --- resolution through keystrokes -----------------------------------------

func TestResolveOursThenSaveWritesDisk(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "o", "s")

	got := readFile(t, dir, "conflict.py")
	if strings.Contains(got, "<<<<<<<") || strings.Contains(got, "=======") || strings.Contains(got, ">>>>>>>") {
		t.Errorf("markers survived on disk:\n%s", got)
	}
	if !strings.Contains(got, "main_value()") {
		t.Errorf("ours content missing:\n%s", got)
	}
	if strings.Contains(got, "feature_value()") {
		t.Errorf("theirs content leaked:\n%s", got)
	}
	// Non-conflict lines must be untouched.
	for _, want := range []string{"import sys", "def main():", "main()"} {
		if !strings.Contains(got, want) {
			t.Errorf("context line %q lost:\n%s", want, got)
		}
	}
	if m.dirty {
		t.Error("still dirty after save")
	}
}

func TestResolveTheirs(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "t", "s")
	got := readFile(t, dir, "conflict.py")
	if !strings.Contains(got, "feature_value()") || strings.Contains(got, "main_value()") {
		t.Errorf("theirs resolution wrong:\n%s", got)
	}
	if !strings.Contains(got, "log(value)") {
		t.Errorf("multi-line theirs side truncated:\n%s", got)
	}
	_ = m
}

func TestResolveBothKeepsOursFirst(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "b", "s")
	got := readFile(t, dir, "conflict.py")
	oi := strings.Index(got, "main_value()")
	ti := strings.Index(got, "feature_value()")
	if oi < 0 || ti < 0 {
		t.Fatalf("both sides not present:\n%s", got)
	}
	if oi > ti {
		t.Errorf("ours should precede theirs:\n%s", got)
	}
	_ = m
}

func TestResolveNoneDropsBlock(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "d", "s")
	got := readFile(t, dir, "conflict.py")
	if strings.Contains(got, "main_value()") || strings.Contains(got, "feature_value()") {
		t.Errorf("discard kept conflict text:\n%s", got)
	}
	if !strings.Contains(got, "import sys") || !strings.Contains(got, "main()") {
		t.Errorf("context lost:\n%s", got)
	}
	_ = m
}

// Numeric aliases must behave identically to the letter keys.
func TestNumericAliases(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "2", "s")
	if got := readFile(t, dir, "conflict.py"); !strings.Contains(got, "feature_value()") {
		t.Errorf("key '2' did not select theirs:\n%s", got)
	}
	_ = m
}

func TestUndoChoiceRestoresMarkersOnSave(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "o", "u", "s")
	got := readFile(t, dir, "conflict.py")
	if !strings.Contains(got, "<<<<<<<") {
		t.Errorf("undo should leave the block unresolved, keeping markers:\n%s", got)
	}
	if m.file.UnresolvedCount() != 1 {
		t.Errorf("UnresolvedCount = %d, want 1", m.file.UnresolvedCount())
	}
}

func TestResolveWithoutConflictUnderCursorWarns(t *testing.T) {
	m, _ := newModel(t)
	// Move to the very first row, which is a context line.
	press(t, m, "g")
	press(t, m, "o")
	if m.statusKind != statusWarning {
		t.Errorf("statusKind = %v, want a warning", m.statusKind)
	}
	if m.file.Blocks[0].Choice.Resolved() {
		t.Error("a context-line keypress resolved a block")
	}
}

// --- commit flow ------------------------------------------------------------

func TestCommitCompletesMergeForReal(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "b", "s")

	press(t, m, "c")
	if m.modal != modalCommit {
		t.Fatalf("modal = %v, want the commit prompt", m.modal)
	}
	// git's prepared message should seed the field.
	if v := m.commitInput.Value(); !strings.Contains(v, "feature") {
		t.Errorf("commit message seed = %q", v)
	}
	press(t, m, "enter")

	// The merge must really be finished according to git itself.
	repo, err := gitx.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	st, err := repo.Status()
	if err != nil {
		t.Fatal(err)
	}
	if st.Merging {
		t.Error("git still reports a merge in progress")
	}
	if len(st.Conflicted) != 0 {
		t.Errorf("conflicts remain: %v", st.Conflicted)
	}
	log, err := repo.Log(5)
	if err != nil || len(log) == 0 {
		t.Fatalf("log: %v %v", log, err)
	}
	if !log[0].Merge {
		t.Error("HEAD is not a merge commit")
	}
	// After committing, the tool should show history so the result is visible.
	if m.view != viewHistory {
		t.Errorf("view = %v, want the history view after commit", m.view)
	}
	if m.statusKind != statusSuccess {
		t.Errorf("statusKind = %v after commit", m.statusKind)
	}
}

func TestCommitRefusedWhileUnresolved(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "c")
	if m.modal == modalCommit {
		t.Error("commit prompt opened with an unresolved conflict")
	}
	if m.statusKind != statusError {
		t.Errorf("statusKind = %v, want an error", m.statusKind)
	}
	// Nothing may have been committed.
	repo, _ := gitx.Open(dir)
	st, _ := repo.Status()
	if !st.Merging {
		t.Error("merge state was disturbed by a refused commit")
	}
}

// Committing with unsaved resolutions must save first, so the commit contains
// what the screen showed rather than stale bytes.
func TestCommitSavesDirtyWorkFirst(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "o") // resolve but do not save
	if !m.dirty {
		t.Fatal("expected dirty state")
	}
	press(t, m, "c")
	if m.modal != modalCommit {
		t.Fatalf("commit prompt did not open, status = %q", m.statusMsg)
	}
	if m.dirty {
		t.Error("still dirty after the commit flow saved")
	}
	if got := readFile(t, dir, "conflict.py"); strings.Contains(got, "<<<<<<<") {
		t.Errorf("file not written before commit:\n%s", got)
	}
}

func TestCommitCancelLeavesMergeIntact(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "o", "s", "c", "esc")
	if m.modal != modalNone {
		t.Error("modal still open after esc")
	}
	repo, _ := gitx.Open(dir)
	st, _ := repo.Status()
	if !st.Merging {
		t.Error("cancelling the prompt should not have committed")
	}
}

func TestCommitRejectsEmptyMessage(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "o", "s", "c")
	m.commitInput.SetValue("")
	press(t, m, "enter")
	if m.modal != modalCommit {
		t.Error("prompt closed on an empty message")
	}
	if m.statusKind != statusError {
		t.Errorf("statusKind = %v, want an error", m.statusKind)
	}
}

// Typed characters must reach the input rather than triggering action keys.
func TestTypingInCommitPromptDoesNotTriggerActions(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "o", "s", "c")
	m.commitInput.SetValue("")
	press(t, m, "c", "o", "s", "q")
	if m.modal != modalCommit {
		t.Fatal("prompt closed while typing")
	}
	if got := m.commitInput.Value(); got != "cosq" {
		t.Errorf("input = %q, want \"cosq\"", got)
	}
}

// --- abort ------------------------------------------------------------------

func TestAbortMergeRestoresTree(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "X")
	if m.modal != modalConfirmAbort {
		t.Fatalf("modal = %v, want the abort confirmation", m.modal)
	}
	press(t, m, "y")

	repo, _ := gitx.Open(dir)
	st, _ := repo.Status()
	if st.Merging {
		t.Error("still merging after abort")
	}
	if got := readFile(t, dir, "conflict.py"); strings.Contains(got, "<<<<<<<") {
		t.Errorf("markers remain after abort:\n%s", got)
	}
}

func TestAbortCancelled(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "X", "n")
	if m.modal != modalNone {
		t.Error("modal still open")
	}
	repo, _ := gitx.Open(dir)
	st, _ := repo.Status()
	if !st.Merging {
		t.Error("abort happened despite cancelling")
	}
}

// --- hand editing -----------------------------------------------------------

func TestEditBlockAppliesCustomText(t *testing.T) {
	m, dir := newModel(t)
	press(t, m, "e")
	if m.modal != modalEditBlock {
		t.Fatalf("modal = %v, want the block editor", m.modal)
	}
	m.blockEdit.SetValue("    value = merged_by_hand()")
	press(t, m, "ctrl+s")
	if m.modal != modalNone {
		t.Error("editor still open after ctrl+s")
	}
	press(t, m, "s")

	got := readFile(t, dir, "conflict.py")
	if !strings.Contains(got, "merged_by_hand()") {
		t.Errorf("hand-edited text missing:\n%s", got)
	}
	if strings.Contains(got, "<<<<<<<") {
		t.Errorf("markers remain:\n%s", got)
	}
	if !strings.Contains(got, "import sys") {
		t.Errorf("context lost:\n%s", got)
	}
}

func TestEditBlockCancelKeepsPreviousChoice(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "o", "e")
	m.blockEdit.SetValue("discarded")
	press(t, m, "esc")
	if m.file.Blocks[0].Choice != conflict.ChoiceOurs {
		t.Errorf("choice = %v, want ours preserved", m.file.Blocks[0].Choice)
	}
}

// --- navigation -------------------------------------------------------------

func TestNextConflictCyclesAndReportsPosition(t *testing.T) {
	dir := newRepo(t)
	// Add a second conflicted region by hand so cycling can be observed.
	src := "a\n<<<<<<< HEAD\nours1\n=======\ntheirs1\n>>>>>>> feature\nb\n" +
		"<<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> feature\nc\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 160, Height: 40})

	if len(m.file.Blocks) != 2 {
		t.Fatalf("want 2 blocks, got %d", len(m.file.Blocks))
	}
	if got := m.lay.rows[m.cursor].blockIndex; got != 0 {
		t.Fatalf("start block = %d", got)
	}
	press(t, m, "n")
	if got := m.lay.rows[m.cursor].blockIndex; got != 1 {
		t.Errorf("after n, block = %d, want 1", got)
	}
	// Wrapping keeps repeated presses useful.
	press(t, m, "n")
	if got := m.lay.rows[m.cursor].blockIndex; got != 0 {
		t.Errorf("n should wrap to 0, got %d", got)
	}
	press(t, m, "p")
	if got := m.lay.rows[m.cursor].blockIndex; got != 1 {
		t.Errorf("p should wrap to 1, got %d", got)
	}

	// Resolving each block independently must not disturb the other.
	m.jumpToBlock(0)
	press(t, m, "o")
	m.jumpToBlock(1)
	press(t, m, "t")
	press(t, m, "s")
	got := readFile(t, dir, "conflict.py")
	want := "a\nours1\nb\ntheirs2\nc\n"
	if got != want {
		t.Errorf("got %q want %q", got, want)
	}
}

func TestCursorMovementStaysInBounds(t *testing.T) {
	m, _ := newModel(t)
	for i := 0; i < 500; i++ {
		press(t, m, "down")
	}
	if m.cursor >= len(m.lay.rows) {
		t.Errorf("cursor %d out of range %d", m.cursor, len(m.lay.rows))
	}
	for i := 0; i < 500; i++ {
		press(t, m, "up")
	}
	if m.cursor != 0 {
		t.Errorf("cursor = %d, want 0", m.cursor)
	}
	press(t, m, "G")
	if m.cursor != len(m.lay.rows)-1 {
		t.Errorf("G did not reach the last row: %d", m.cursor)
	}
	press(t, m, "g")
	if m.cursor != 0 {
		t.Errorf("g did not reach the first row: %d", m.cursor)
	}
}

func TestApplyToAllConflicts(t *testing.T) {
	dir := newRepo(t)
	src := "x\n<<<<<<< HEAD\no1\n=======\nt1\n>>>>>>> f\ny\n<<<<<<< HEAD\no2\n=======\nt2\n>>>>>>> f\nz\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 160, Height: 40})

	press(t, m, "T", "s")
	if got := readFile(t, dir, "conflict.py"); got != "x\nt1\ny\nt2\nz\n" {
		t.Errorf("all-theirs gave %q", got)
	}
	press(t, m, "O", "s")
	if got := readFile(t, dir, "conflict.py"); got != "x\no1\ny\no2\nz\n" {
		t.Errorf("all-ours after reload gave %q", got)
	}
}

// --- panels and views -------------------------------------------------------

func TestPanelFocusCyclesVisibleOnly(t *testing.T) {
	m, _ := newModel(t)
	seen := map[side]bool{}
	for i := 0; i < 6; i++ {
		seen[m.focus] = true
		press(t, m, "]")
	}
	if seen[sideBase] {
		t.Error("focus reached the hidden base panel")
	}
	for _, s := range []side{sideOurs, sideResult, sideTheirs} {
		if !seen[s] {
			t.Errorf("focus never reached %v", s)
		}
	}
	// Showing the base panel adds it to the cycle.
	press(t, m, "v")
	if !m.showBase {
		t.Fatal("v did not show the base panel")
	}
	seen = map[side]bool{}
	for i := 0; i < 8; i++ {
		seen[m.focus] = true
		press(t, m, "]")
	}
	if !seen[sideBase] {
		t.Error("base panel not focusable once shown")
	}
	// Hiding it must not strand focus.
	m.focus = sideBase
	press(t, m, "v")
	if m.focus == sideBase {
		t.Error("focus left on the hidden base panel")
	}
}

func TestViewSwitching(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "L")
	if m.view != viewHistory {
		t.Errorf("L → %v", m.view)
	}
	press(t, m, "f")
	if m.view != viewFiles {
		t.Errorf("f → %v", m.view)
	}
	press(t, m, "m")
	if m.view != viewMerge {
		t.Errorf("m → %v", m.view)
	}
	press(t, m, "?")
	if m.view != viewHelp {
		t.Errorf("? → %v", m.view)
	}
	press(t, m, "?")
	if m.view != viewMerge {
		t.Error("? did not toggle help off")
	}
}

func TestHistoryShowsRealCommits(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "L")
	if len(m.log) < 2 {
		t.Fatalf("log has %d entries", len(m.log))
	}
	out := m.View()
	if !strings.Contains(out, "main change") {
		t.Errorf("history view missing a commit subject:\n%s", out)
	}
	// Selection must move and stay in range.
	press(t, m, "down", "down", "down", "down", "down")
	if m.logCursor >= len(m.log) {
		t.Errorf("logCursor %d out of range %d", m.logCursor, len(m.log))
	}
}

func TestFilesViewListsConflicts(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "f")
	if len(m.conflictFiles) != 1 || m.conflictFiles[0] != "conflict.py" {
		t.Errorf("conflictFiles = %v", m.conflictFiles)
	}
	if out := m.View(); !strings.Contains(out, "conflict.py") {
		t.Errorf("files view missing the path:\n%s", out)
	}
}

func TestQuitKey(t *testing.T) {
	m, _ := newModel(t)
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("q")})
	if cmd == nil {
		t.Fatal("q returned no command")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Error("q did not quit")
	}
}

// --- rendering --------------------------------------------------------------

// The three-way panels must all be present in one screen, with both sides'
// conflict text visible at the same time.
func TestMergeViewShowsAllThreePanelsAndBothSides(t *testing.T) {
	m, _ := newModel(t)
	out := m.View()
	for _, want := range []string{"OURS", "RESULT", "THEIRS"} {
		if !strings.Contains(out, want) {
			t.Errorf("panel %q missing from the view:\n%s", want, out)
		}
	}
	// Both conflicting texts on the same snapshot.
	if !strings.Contains(out, "main_value()") {
		t.Error("ours text not visible")
	}
	if !strings.Contains(out, "feature_value()") {
		t.Error("theirs text not visible")
	}
	// And on the same physical line, which is the point of the alignment.
	for _, line := range strings.Split(out, "\n") {
		if strings.Contains(line, "main_value()") && strings.Contains(line, "feature_value()") {
			return
		}
	}
	t.Errorf("ours and theirs are not on a shared row:\n%s", out)
}

// Conflict text must carry a background highlight, not merely a foreground
// colour, so the region is unmistakable.
func TestConflictLinesUseBackgroundHighlight(t *testing.T) {
	m, _ := newModel(t)
	out := m.View()
	var found bool
	for _, line := range strings.Split(out, "\n") {
		if !strings.Contains(line, "main_value()") {
			continue
		}
		// A 48;2 SGR sequence is a truecolor background.
		if strings.Contains(line, "48;2;") || strings.Contains(line, "48;5;") {
			found = true
		}
	}
	if !found {
		t.Error("no background highlight on conflict content")
	}
	// Context lines must not be highlighted, or the distinction is lost.
	for _, line := range strings.Split(out, "\n") {
		if strings.Contains(line, "import sys") &&
			(strings.Contains(line, "48;2;") || strings.Contains(line, "48;5;")) {
			// The gutter and borders are unstyled, so a background here means
			// the context row itself was highlighted.
			t.Errorf("context line carries a background highlight: %q", line)
		}
	}
}

func TestViewFitsWindowExactly(t *testing.T) {
	m, _ := newModel(t)
	for _, size := range [][2]int{{160, 40}, {100, 30}, {80, 24}, {60, 20}, {200, 50}} {
		m.Update(tea.WindowSizeMsg{Width: size[0], Height: size[1]})
		out := m.View()
		lines := strings.Split(out, "\n")
		if len(lines) > size[1] {
			t.Errorf("%dx%d: rendered %d lines, exceeds the window height",
				size[0], size[1], len(lines))
		}
		for i, l := range lines {
			if w := lipgloss.Width(l); w > size[0] {
				t.Errorf("%dx%d: line %d is %d wide, exceeds the window width",
					size[0], size[1], i, w)
			}
		}
	}
}

// Every view must render at small sizes without panicking or overflowing.
func TestAllViewsRenderAtManySizes(t *testing.T) {
	m, _ := newModel(t)
	views := []struct {
		name string
		keys []string
	}{
		{"merge", []string{"m"}},
		{"history", []string{"L"}},
		{"files", []string{"f"}},
		{"help", []string{"?"}},
	}
	sizes := [][2]int{{40, 12}, {80, 24}, {120, 35}, {200, 60}}
	for _, v := range views {
		press(t, m, v.keys...)
		for _, s := range sizes {
			m.Update(tea.WindowSizeMsg{Width: s[0], Height: s[1]})
			out := m.View()
			if out == "" {
				t.Errorf("%s at %dx%d rendered nothing", v.name, s[0], s[1])
			}
			for i, l := range strings.Split(out, "\n") {
				if w := lipgloss.Width(l); w > s[0] {
					t.Errorf("%s at %dx%d: line %d width %d > %d",
						v.name, s[0], s[1], i, w, s[0])
				}
			}
		}
	}
}

// The footer and help must document the keys, which is the discoverability
// requirement.
func TestKeysAreDiscoverable(t *testing.T) {
	m, _ := newModel(t)
	footer := m.View()
	for _, want := range []string{"ours", "theirs", "both", "save", "commit", "help"} {
		if !strings.Contains(footer, want) {
			t.Errorf("footer does not mention %q:\n%s", want, footer)
		}
	}

	press(t, m, "?")
	m.Update(tea.WindowSizeMsg{Width: 160, Height: 60})
	help := m.View()
	for _, want := range []string{
		"KEYBOARD REFERENCE", "Navigate", "Resolve current conflict",
		"Git actions", "next conflict", "take ours", "take theirs",
		"keep both", "commit merge", "abort merge",
	} {
		if !strings.Contains(help, want) {
			t.Errorf("help missing %q", want)
		}
	}
}

// Every binding in the key map must be reachable from the help screen, so no
// feature is undocumented.
func TestEveryBindingIsDocumented(t *testing.T) {
	k := newKeyMap()
	documented := map[string]bool{}
	for _, sec := range k.sections() {
		for _, b := range sec.Keys {
			documented[b.Help().Key] = true
		}
	}
	// Compare against the exhaustive list of bindings on the struct.
	bindings := map[string]string{
		"Up": k.Up.Help().Key, "Down": k.Down.Help().Key, "Left": k.Left.Help().Key,
		"Right": k.Right.Help().Key, "PageUp": k.PageUp.Help().Key,
		"PageDown": k.PageDown.Help().Key, "Home": k.Home.Help().Key, "End": k.End.Help().Key,
		"NextConflict": k.NextConflict.Help().Key, "PrevConflict": k.PrevConflict.Help().Key,
		"Ours": k.Ours.Help().Key, "Theirs": k.Theirs.Help().Key, "Both": k.Both.Help().Key,
		"None": k.None.Help().Key, "Clear": k.Clear.Help().Key,
		"AllOurs": k.AllOurs.Help().Key, "AllTheirs": k.AllTheirs.Help().Key,
		"AllBoth": k.AllBoth.Help().Key, "NextPanel": k.NextPanel.Help().Key,
		"PrevPanel": k.PrevPanel.Help().Key, "ToggleBase": k.ToggleBase.Help().Key,
		"Save": k.Save.Help().Key, "Commit": k.Commit.Help().Key, "Reload": k.Reload.Help().Key,
		"Edit": k.Edit.Help().Key, "Abort": k.Abort.Help().Key, "History": k.History.Help().Key,
		"Files": k.Files.Help().Key, "Merge": k.Merge.Help().Key,
		"Confirm": k.Confirm.Help().Key, "Cancel": k.Cancel.Help().Key,
		"Help": k.Help.Help().Key, "Quit": k.Quit.Help().Key,
	}
	for name, keyLabel := range bindings {
		if keyLabel == "" {
			t.Errorf("binding %s has no help label", name)
			continue
		}
		if !documented[keyLabel] {
			t.Errorf("binding %s (%s) is not on the help screen", name, keyLabel)
		}
	}
}

func TestHeaderShowsProgressAndBranch(t *testing.T) {
	m, _ := newModel(t)
	out := m.View()
	if !strings.Contains(out, "main") {
		t.Error("header does not show the branch")
	}
	if !strings.Contains(out, "MERGING") {
		t.Error("header does not show the merge state")
	}
	if !strings.Contains(out, "resolved 0/1") {
		t.Errorf("header does not show progress:\n%s", out)
	}
	press(t, m, "o")
	if out := m.View(); !strings.Contains(out, "resolved 1/1") {
		t.Errorf("progress did not update:\n%s", out)
	}
	if !strings.Contains(m.View(), "UNSAVED") {
		t.Error("unsaved state not indicated")
	}
}

// --- reload and robustness --------------------------------------------------

func TestReloadDiscardsUnsavedChoices(t *testing.T) {
	m, _ := newModel(t)
	press(t, m, "o")
	if !m.dirty {
		t.Fatal("expected dirty")
	}
	press(t, m, "r")
	if m.dirty {
		t.Error("still dirty after reload")
	}
	if m.file.UnresolvedCount() != 1 {
		t.Errorf("reload did not restore the unresolved block: %d", m.file.UnresolvedCount())
	}
}

// External edits to the file must be picked up, since the tool is not the only
// thing that can touch the working tree.
func TestReloadPicksUpExternalChanges(t *testing.T) {
	m, dir := newModel(t)
	src := "only\n<<<<<<< HEAD\nnewours\n=======\nnewtheirs\n>>>>>>> f\ntail\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	press(t, m, "r")
	if len(m.file.Blocks) != 1 {
		t.Fatalf("blocks = %d", len(m.file.Blocks))
	}
	if strings.Join(m.file.Blocks[0].Ours, "") != "newours" {
		t.Errorf("ours = %v", m.file.Blocks[0].Ours)
	}
}

func TestMalformedFileReportsErrorNotPanic(t *testing.T) {
	dir := newRepo(t)
	// An unterminated block: the parser must refuse rather than guess.
	bad := "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(bad), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	if m.loadErr == nil {
		t.Error("loadErr = nil for a malformed file")
	}
	if out := m.View(); !strings.Contains(out, "Cannot open") {
		t.Errorf("view does not report the parse failure:\n%s", out)
	}
	// Keys must not panic in this state.
	press(t, m, "o", "s", "n", "c", "down", "e")
}

func TestFileWithoutMarkersIsHandled(t *testing.T) {
	dir := newRepo(t)
	if err := os.WriteFile(filepath.Join(dir, "clean.py"), []byte("print(1)\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "clean.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	out := m.View()
	if !strings.Contains(out, "No conflict markers") {
		t.Errorf("view does not explain the absence of conflicts:\n%s", out)
	}
	press(t, m, "o", "n", "e") // must not panic
}

// Wide runes and long lines must not break the column alignment.
func TestWideCharactersAndLongLines(t *testing.T) {
	dir := newRepo(t)
	long := strings.Repeat("x", 400)
	src := "prefix English-only textテキスト\n<<<<<<< HEAD\n" + long + "\n=======\nEnglish-only text ours\n>>>>>>> f\nend\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	for _, s := range [][2]int{{80, 24}, {120, 30}} {
		m.Update(tea.WindowSizeMsg{Width: s[0], Height: s[1]})
		for i, l := range strings.Split(m.View(), "\n") {
			if w := lipgloss.Width(l); w > s[0] {
				t.Errorf("%dx%d line %d width %d > %d", s[0], s[1], i, w, s[0])
			}
		}
	}
	// Horizontal scrolling must stay in bounds too.
	press(t, m, "right", "right", "right", "right")
	for i, l := range strings.Split(m.View(), "\n") {
		if w := lipgloss.Width(l); w > 120 {
			t.Errorf("after scrolling, line %d width %d > 120", i, w)
		}
	}
	press(t, m, "left", "left", "left", "left", "left", "left")
	if m.hoff != 0 {
		t.Errorf("hoff = %d, want 0", m.hoff)
	}
}

// Control characters in the file must be neutralised, not written to the
// terminal raw.
func TestControlCharactersSanitised(t *testing.T) {
	dir := newRepo(t)
	src := "a\x07b\n<<<<<<< HEAD\nours\x1b[31m\n=======\ntheirs\n>>>>>>> f\nc\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	out := m.View()
	if strings.Contains(out, "\x07") {
		t.Error("BEL reached the output")
	}
	if strings.Contains(out, "\x1b[31m") {
		t.Error("raw escape sequence from file content reached the output")
	}
	// The file itself must round-trip unchanged when written back.
	press(t, m, "o", "s")
	if got := readFile(t, dir, "conflict.py"); !strings.Contains(got, "a\x07b") {
		t.Error("sanitisation must be display-only, not applied to the file")
	}
}

// A zero-size window can occur during startup; rendering must survive it.
func TestTinyWindow(t *testing.T) {
	m, _ := newModel(t)
	for _, s := range [][2]int{{1, 1}, {5, 3}, {20, 6}, {0, 0}} {
		m.Update(tea.WindowSizeMsg{Width: s[0], Height: s[1]})
		_ = m.View()
	}
}

func TestEmptyConflictSide(t *testing.T) {
	dir := newRepo(t)
	// theirs adds lines, ours has none: a pure addition.
	src := "head\n<<<<<<< HEAD\n=======\nadded1\nadded2\n>>>>>>> f\ntail\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 30})
	if out := m.View(); !strings.Contains(out, "added1") {
		t.Errorf("added lines not shown:\n%s", out)
	}
	press(t, m, "o", "s")
	if got := readFile(t, dir, "conflict.py"); got != "head\ntail\n" {
		t.Errorf("got %q", got)
	}
}

// --- multi-file merges ------------------------------------------------------

// newMultiRepo leaves three files conflicted, which is what exposes staging
// bugs: resolving one file must not lose the others, and the merge cannot be
// committed until all of them are resolved.
func newMultiRepo(t *testing.T) string {
	t.Helper()
	if err := gitx.Available(); err != nil {
		t.Skip("git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_AUTHOR_NAME=T", "GIT_AUTHOR_EMAIL=anonymous@example.invalid",
			"GIT_COMMITTER_NAME=T", "GIT_COMMITTER_EMAIL=anonymous@example.invalid",
			"GIT_CONFIG_GLOBAL=/dev/null", "GIT_CONFIG_SYSTEM=/dev/null", "LC_ALL=C",
		)
		if out, err := cmd.CombinedOutput(); err != nil && args[0] != "merge" {
			t.Fatalf("git %s: %v\n%s", strings.Join(args, " "), err, out)
		}
	}
	files := func(a, b, c string) {
		t.Helper()
		if err := os.MkdirAll(filepath.Join(dir, "sub"), 0o755); err != nil {
			t.Fatal(err)
		}
		for name, body := range map[string]string{
			"conflict.py": "top\n" + a + "\nbottom\n",
			"other.txt":   "top\n" + b + "\nbottom\n",
			"sub/deep.go": "top\n" + c + "\nbottom\n",
		} {
			if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
				t.Fatal(err)
			}
		}
	}

	run("init", "-q", "-b", "main")
	run("config", "user.name", "T")
	run("config", "user.email", "anonymous@example.invalid")
	run("config", "commit.gpgsign", "false")
	files("base1", "base2", "base3")
	run("add", ".")
	run("commit", "-q", "-m", "init")
	run("checkout", "-q", "-b", "feature")
	files("feat1", "feat2", "feat3")
	run("commit", "-q", "-am", "feature")
	run("checkout", "-q", "main")
	files("main1", "main2", "main3")
	run("commit", "-q", "-am", "main")
	run("merge", "feature")
	return dir
}

// Saving a fully resolved file must stage it, because `git add` is what marks a
// conflict resolved in git. Otherwise the path stays unmerged and the eventual
// commit is refused.
func TestSaveStagesResolvedFile(t *testing.T) {
	dir := newMultiRepo(t)
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 150, Height: 40})

	press(t, m, "o", "s")

	repo, err := gitx.Open(dir)
	if err != nil {
		t.Fatal(err)
	}
	files, err := repo.ConflictedFiles()
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if f == "conflict.py" {
			t.Error("conflict.py still unmerged after save; it was not staged")
		}
	}
	if len(files) != 2 {
		t.Errorf("remaining conflicts = %v, want the other two files", files)
	}
}

// A partially resolved file must NOT be staged, since staging it would tell git
// the conflict is settled while markers are still in the file.
func TestPartiallyResolvedFileIsNotStaged(t *testing.T) {
	dir := newMultiRepo(t)
	src := "a\n<<<<<<< HEAD\no1\n=======\nt1\n>>>>>>> f\nb\n<<<<<<< HEAD\no2\n=======\nt2\n>>>>>>> f\nc\n"
	if err := os.WriteFile(filepath.Join(dir, "conflict.py"), []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 150, Height: 40})

	press(t, m, "o", "s") // resolve only the first of two blocks
	if m.statusKind != statusWarning {
		t.Errorf("statusKind = %v, want a warning about the remaining conflict", m.statusKind)
	}
	repo, _ := gitx.Open(dir)
	files, _ := repo.ConflictedFiles()
	var still bool
	for _, f := range files {
		if f == "conflict.py" {
			still = true
		}
	}
	if !still {
		t.Error("a half-resolved file was staged as resolved")
	}
	// The markers for the untouched block must survive on disk.
	if got := readFile(t, dir, "conflict.py"); !strings.Contains(got, "<<<<<<<") {
		t.Errorf("markers for the unresolved block were lost:\n%s", got)
	}
}

// Committing must be refused, with a useful message, while other files are
// still conflicted.
func TestCommitBlockedByOtherConflictedFiles(t *testing.T) {
	dir := newMultiRepo(t)
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 150, Height: 40})

	press(t, m, "o", "s", "c")
	if m.modal == modalCommit {
		t.Fatal("commit prompt opened while other files are conflicted")
	}
	if m.statusKind != statusError {
		t.Errorf("statusKind = %v, want an error", m.statusKind)
	}
	// The message should name the outstanding files so the user knows what to do.
	for _, want := range []string{"other.txt", "sub/deep.go"} {
		if !strings.Contains(m.statusMsg, want) {
			t.Errorf("status %q does not mention %s", m.statusMsg, want)
		}
	}
}

// The whole multi-file flow: resolve each file through the file list, then
// commit once, and confirm git really finished the merge with every resolution.
func TestResolveAllFilesThenCommit(t *testing.T) {
	dir := newMultiRepo(t)
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 150, Height: 40})

	// conflict.py -> ours, other.txt -> theirs, sub/deep.go -> both.
	press(t, m, "o", "s")
	if cmd := m.openFile("other.txt"); cmd != nil {
		drain(t, m, cmd)
	}
	press(t, m, "t", "s")
	if cmd := m.openFile("sub/deep.go"); cmd != nil {
		drain(t, m, cmd)
	}
	press(t, m, "b", "s")

	press(t, m, "c")
	if m.modal != modalCommit {
		t.Fatalf("commit prompt did not open; status = %q", m.statusMsg)
	}
	press(t, m, "enter")

	repo, _ := gitx.Open(dir)
	st, err := repo.Status()
	if err != nil {
		t.Fatal(err)
	}
	if st.Merging {
		t.Error("merge not finished")
	}
	if len(st.Conflicted) != 0 {
		t.Errorf("conflicts remain: %v", st.Conflicted)
	}

	// Each file must carry the strategy chosen for it, with markers gone.
	checks := map[string][]string{
		"conflict.py": {"main1"},
		"other.txt":   {"feat2"},
		"sub/deep.go": {"main3", "feat3"},
	}
	for path, wants := range checks {
		got, err := repo.Run("show", "HEAD:"+path)
		if err != nil {
			t.Fatalf("show %s: %v", path, err)
		}
		for _, w := range wants {
			if !strings.Contains(got, w) {
				t.Errorf("%s missing %q:\n%s", path, w, got)
			}
		}
		if strings.Contains(got, "<<<<<<<") || strings.Contains(got, ">>>>>>>") {
			t.Errorf("%s committed with markers:\n%s", path, got)
		}
		if !strings.Contains(got, "top") || !strings.Contains(got, "bottom") {
			t.Errorf("%s lost its context lines:\n%s", path, got)
		}
	}
	// conflict.py chose ours, so the incoming text must be absent.
	if got, _ := repo.Run("show", "HEAD:conflict.py"); strings.Contains(got, "feat1") {
		t.Errorf("ours-resolution leaked theirs content:\n%s", got)
	}
}

// Switching files with unsaved work must refuse rather than silently discard it.
func TestOpenFileRefusesToDiscardUnsavedWork(t *testing.T) {
	dir := newMultiRepo(t)
	m, err := New(Config{WorkDir: dir, File: "conflict.py"})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 150, Height: 40})

	press(t, m, "o") // dirty, unsaved
	if cmd := m.openFile("other.txt"); cmd != nil {
		drain(t, m, cmd)
	}
	if m.relPath != "conflict.py" {
		t.Errorf("switched away with unsaved changes; relPath = %q", m.relPath)
	}
	if m.statusKind != statusWarning {
		t.Errorf("statusKind = %v, want a warning", m.statusKind)
	}
}
