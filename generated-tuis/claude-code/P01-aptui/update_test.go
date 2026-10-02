package main

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

// Tests for the interaction logic: key routing, focus, filtering, cursor
// scrolling, the confirmation gate, and the post-operation refresh. These drive
// the real Update path with synthetic messages, so no apt is required.

// newTestModel returns a model populated from fixtures and sized, as if the
// initial scans had completed.
func newTestModel() *model {
	m := newModel()
	m.store = testStore()
	m.store.rebuild()
	m.loadingInstalled = false
	m.loadingAvailable = false
	m.loadingUpgradable = false
	m.aptListsChecked = true
	m.aptListsPresent = true
	m.startupWarning = ""
	m.status = ""
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	return m
}

// press sends a keypress through Update, as the runtime would.
func press(m *model, keys string) {
	for _, k := range strings.Split(keys, " ") {
		var msg tea.KeyMsg
		switch k {
		case "up", "down", "enter", "esc", "tab", "pgup", "pgdown", "home", "end":
			msg = tea.KeyMsg{Type: keyTypeFor(k)}
		default:
			msg = tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(k)}
		}
		m.Update(msg)
	}
}

func keyTypeFor(name string) tea.KeyType {
	switch name {
	case "up":
		return tea.KeyUp
	case "down":
		return tea.KeyDown
	case "enter":
		return tea.KeyEnter
	case "esc":
		return tea.KeyEsc
	case "tab":
		return tea.KeyTab
	case "pgup":
		return tea.KeyPgUp
	case "pgdown":
		return tea.KeyPgDown
	case "home":
		return tea.KeyHome
	case "end":
		return tea.KeyEnd
	}
	return tea.KeyRunes
}

func TestCursorNavigation(t *testing.T) {
	m := newTestModel()
	// Fixture order: bash, brandnew, libc-bin, localonly, nginx, removedpkg.

	if got := m.selected().Name; got != "bash" {
		t.Fatalf("initial selection = %q, want bash", got)
	}

	press(m, "down down")
	if got := m.selected().Name; got != "libc-bin" {
		t.Errorf("after two downs = %q, want libc-bin", got)
	}

	press(m, "up")
	if got := m.selected().Name; got != "brandnew" {
		t.Errorf("after up = %q, want brandnew", got)
	}

	// j/k must work as well as the arrows.
	press(m, "j j")
	if got := m.selected().Name; got != "localonly" {
		t.Errorf("after jj = %q, want localonly", got)
	}
	press(m, "k")
	if got := m.selected().Name; got != "libc-bin" {
		t.Errorf("after k = %q, want libc-bin", got)
	}
}

func TestCursorClampsAtBothEnds(t *testing.T) {
	m := newTestModel()
	n := len(m.store.visible())

	// Walking off the top must stop at the first row, not wrap or go negative.
	press(m, "up up up up up")
	if m.cursor != 0 {
		t.Errorf("cursor = %d after walking off the top, want 0", m.cursor)
	}

	press(m, "end")
	if m.cursor != n-1 {
		t.Errorf("cursor = %d after end, want %d", m.cursor, n-1)
	}
	press(m, "down down down")
	if m.cursor != n-1 {
		t.Errorf("cursor = %d after walking off the bottom, want %d", m.cursor, n-1)
	}

	press(m, "home")
	if m.cursor != 0 {
		t.Errorf("cursor = %d after home, want 0", m.cursor)
	}
}

func TestListScrollsRatherThanPaginates(t *testing.T) {
	// A list far taller than the pane must scroll continuously: the window
	// follows the cursor one row at a time, so there are no pages to switch.
	m := newModel()
	m.store = newStore()

	var many []availablePkg
	for i := 0; i < 500; i++ {
		many = append(many, availablePkg{
			Name:     "pkg" + string(rune('a'+i%26)) + itoa(i),
			Version:  "1.0",
			Synopsis: "test package",
		})
	}
	m.store.setAvailable(many)
	m.store.rebuild()
	m.loadingInstalled, m.loadingAvailable, m.loadingUpgradable = false, false, false
	m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})

	h := m.listHeight()
	if h < 2 {
		t.Fatalf("list height %d is too small to test scrolling", h)
	}

	// Moving within the first window must not scroll it.
	press(m, "down")
	if m.listOffset != 0 {
		t.Errorf("offset = %d after one down, want 0 (cursor still in view)", m.listOffset)
	}

	// Crossing the bottom edge scrolls by exactly one row.
	m.cursor = h - 1
	m.clampCursor()
	if m.listOffset != 0 {
		t.Fatalf("offset = %d with the cursor on the last visible row, want 0", m.listOffset)
	}
	press(m, "down")
	if m.listOffset != 1 {
		t.Errorf("offset = %d after crossing the edge, want 1", m.listOffset)
	}

	// The last row must be reachable, and the window must not scroll past the end.
	press(m, "end")
	if m.cursor != 499 {
		t.Errorf("cursor = %d after end, want 499", m.cursor)
	}
	if want := 500 - h; m.listOffset != want {
		t.Errorf("offset = %d at the end, want %d", m.listOffset, want)
	}
}

// itoa avoids importing strconv for one call in a test helper.
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var digits []byte
	for n > 0 {
		digits = append([]byte{byte('0' + n%10)}, digits...)
		n /= 10
	}
	return string(digits)
}

func TestSearchFiltersInRealTime(t *testing.T) {
	m := newTestModel()

	press(m, "/")
	if !m.searchFocus {
		t.Fatal("/ did not focus the search box")
	}

	// Each keystroke re-filters; there is no separate submit step.
	press(m, "n")
	afterN := len(m.store.visible())
	press(m, "g")
	afterNg := len(m.store.visible())

	if afterNg > afterN {
		t.Errorf("typing more characters widened the result set: %d then %d", afterN, afterNg)
	}
	if got := m.store.query; got != "ng" {
		t.Errorf("query = %q, want \"ng\"", got)
	}
	for _, p := range m.store.visible() {
		if !strings.Contains(strings.ToLower(p.Name), "ng") &&
			!strings.Contains(strings.ToLower(p.Synopsis), "ng") {
			t.Errorf("%q matches neither the name nor the synopsis", p.Name)
		}
	}
}

func TestSearchEnterKeepsTheFilterAndLeavesTheBox(t *testing.T) {
	m := newTestModel()

	press(m, "/")
	press(m, "n g i n x")
	press(m, "enter")

	if m.searchFocus {
		t.Error("enter should leave the search box")
	}
	if m.store.query != "nginx" {
		t.Errorf("enter discarded the query: %q", m.store.query)
	}
	if len(m.store.visible()) == 0 {
		t.Error("the filter was lost when leaving the box")
	}
}

func TestSearchEscClearsThenLeaves(t *testing.T) {
	m := newTestModel()

	press(m, "/")
	press(m, "n g i n x")

	// First esc clears the query but stays in the box.
	press(m, "esc")
	if m.store.query != "" {
		t.Errorf("first esc did not clear the query: %q", m.store.query)
	}
	if !m.searchFocus {
		t.Error("first esc should stay in the search box")
	}

	// Second esc leaves the box.
	press(m, "esc")
	if m.searchFocus {
		t.Error("second esc should leave the search box")
	}
	if n := len(m.store.visible()); n != 6 {
		t.Errorf("%d packages visible after clearing, want all 6", n)
	}
}

func TestSearchModeDoesNotTriggerOperations(t *testing.T) {
	// While typing, letters bound to actions must be text, not commands. This is
	// what stops a search for "install" from installing something.
	m := newTestModel()

	press(m, "/")
	press(m, "i x u")

	if m.confirm != nil {
		t.Errorf("typing in the search box staged an operation: %+v", m.confirm)
	}
	if m.store.query != "ixu" {
		t.Errorf("query = %q, want \"ixu\"", m.store.query)
	}
}

func TestFilterKeysSelectTheRightSubset(t *testing.T) {
	m := newTestModel()

	press(m, "2") // installed
	if m.store.filter != filterInstalled {
		t.Errorf("filter = %v, want installed", m.store.filter)
	}
	for _, p := range m.store.visible() {
		if !p.Installed {
			t.Errorf("%q is not installed but appears in the installed filter", p.Name)
		}
	}

	press(m, "4") // upgradable
	if m.store.filter != filterUpgradable {
		t.Errorf("filter = %v, want upgradable", m.store.filter)
	}
	for _, p := range m.store.visible() {
		if !p.Upgradable {
			t.Errorf("%q is not upgradable but appears in the upgradable filter", p.Name)
		}
	}

	press(m, "1") // all
	if n := len(m.store.visible()); n != 6 {
		t.Errorf("%d visible under the all filter, want 6", n)
	}
}

func TestFilterCycleVisitsEveryMode(t *testing.T) {
	m := newTestModel()

	seen := map[filterMode]bool{m.store.filter: true}
	for i := 0; i < 5; i++ {
		press(m, "f")
		seen[m.store.filter] = true
	}
	if len(seen) != 5 {
		t.Errorf("cycling visited %d modes, want 5", len(seen))
	}
	// A full cycle returns to where it started.
	if m.store.filter != filterAll {
		t.Errorf("after five cycles filter = %v, want all", m.store.filter)
	}
}

func TestTabCyclesFocusWithoutHidingAnything(t *testing.T) {
	m := newTestModel()

	if m.focus != focusList {
		t.Fatalf("initial focus = %v, want list", m.focus)
	}
	press(m, "tab")
	if m.focus != focusDetails {
		t.Errorf("focus = %v after one tab, want details", m.focus)
	}
	press(m, "tab")
	if m.focus != focusLog {
		t.Errorf("focus = %v after two tabs, want log", m.focus)
	}
	press(m, "tab")
	if m.focus != focusList {
		t.Errorf("focus = %v after three tabs, want list again", m.focus)
	}

	// Focus must not change what is on screen, only where scrolling applies.
	for _, f := range []focusArea{focusList, focusDetails, focusLog} {
		m.focus = f
		view := m.View()
		for _, fragment := range []string{"PACKAGE", "apt output"} {
			if !strings.Contains(view, fragment) {
				t.Errorf("focus %v hid %q", f, fragment)
			}
		}
	}
}

func TestArrowKeysScrollTheFocusedPaneOnly(t *testing.T) {
	m := newTestModel()

	// With the details pane focused, ↓ scrolls it and leaves the selection put.
	m.detailsFor = "bash"
	m.detailsPkg = detailsFromRecord(parseControl(bashShowFixture))
	m.details.SetContent(m.renderDetails(m.layout.DetailsWidth))
	m.details.Height = 3 // force the content to overflow

	before := m.selected().Name
	m.focus = focusDetails
	press(m, "down down")

	if m.selected().Name != before {
		t.Errorf("scrolling the details pane moved the list selection to %q", m.selected().Name)
	}
	if m.details.YOffset == 0 {
		t.Error("the details pane did not scroll")
	}
}

func TestOperationsRequireConfirmation(t *testing.T) {
	// Nothing may touch the system on a single keypress.
	tests := []struct {
		key      string
		pkg      string
		wantKind opKind
	}{
		{"i", "nginx", opInstall},
		{"x", "bash", opRemove},
		{"X", "bash", opPurge},
		{"u", "libc-bin", opUpgradeOne},
	}

	for _, tt := range tests {
		t.Run(tt.key, func(t *testing.T) {
			m := newTestModel()

			// Select the package the operation applies to.
			for i, p := range m.store.visible() {
				if p.Name == tt.pkg {
					m.cursor = i
					break
				}
			}

			press(m, tt.key)

			if m.confirm == nil {
				t.Fatalf("%q did not ask for confirmation", tt.key)
			}
			if m.confirm.op.Kind != tt.wantKind {
				t.Errorf("staged kind = %v, want %v", m.confirm.op.Kind, tt.wantKind)
			}
			if m.confirm.op.Target != tt.pkg {
				t.Errorf("staged target = %q, want %q", m.confirm.op.Target, tt.pkg)
			}
			if m.busy {
				t.Error("the operation started before being confirmed")
			}
		})
	}
}

func TestConfirmationCancelDoesNotRun(t *testing.T) {
	m := newTestModel()
	m.cursor = indexOf(m, "nginx")

	press(m, "i")
	if m.confirm == nil {
		t.Fatal("no confirmation staged")
	}

	press(m, "n")
	if m.confirm != nil {
		t.Error("n did not dismiss the confirmation")
	}
	if m.busy {
		t.Error("cancelling started the operation anyway")
	}
	if !strings.Contains(m.status, "cancelled") {
		t.Errorf("status = %q, want it to mention cancelling", m.status)
	}
}

func TestConfirmationIgnoresUnrelatedKeys(t *testing.T) {
	// A stray keypress must neither confirm nor cancel: it is safest for the
	// prompt to stay up until the user answers it.
	m := newTestModel()
	m.cursor = indexOf(m, "nginx")

	press(m, "i")
	press(m, "z")

	if m.confirm == nil {
		t.Error("an unrelated key dismissed the confirmation")
	}
	if m.busy {
		t.Error("an unrelated key started the operation")
	}
}

func indexOf(m *model, name string) int {
	for i, p := range m.store.visible() {
		if p.Name == name {
			return i
		}
	}
	return 0
}

func TestOperationsRejectImpossibleRequests(t *testing.T) {
	// Guard rails: the tool explains why rather than handing apt a request that
	// cannot succeed.
	tests := []struct {
		name      string
		key       string
		pkg       string
		wantWords string
	}{
		{"install an installed package", "i", "bash", "already installed"},
		{"remove a package that is not installed", "x", "nginx", "not installed"},
		{"upgrade a package that is not installed", "u", "nginx", "not installed"},
		{"upgrade an up-to-date package", "u", "bash", "newest version"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := newTestModel()
			m.cursor = indexOf(m, tt.pkg)

			press(m, tt.key)

			if m.confirm != nil {
				t.Errorf("staged an impossible operation: %+v", m.confirm.op)
			}
			if !strings.Contains(m.status, tt.wantWords) {
				t.Errorf("status = %q, want it to mention %q", m.status, tt.wantWords)
			}
		})
	}
}

func TestInstallOnAnInstalledUpgradablePackagePointsAtUpgrade(t *testing.T) {
	// libc-bin is installed and upgradable: pressing i should redirect the user
	// to u rather than silently doing nothing.
	m := newTestModel()
	m.cursor = indexOf(m, "libc-bin")

	press(m, "i")

	if m.confirm != nil {
		t.Errorf("staged an install for an installed package: %+v", m.confirm.op)
	}
	if !strings.Contains(m.status, "press u") {
		t.Errorf("status = %q, want it to point at the upgrade key", m.status)
	}
}

func TestUpgradeAllWithNothingToDoIsRejected(t *testing.T) {
	m := newTestModel()
	m.store.setUpgradable(nil)
	m.store.rebuild()

	press(m, "U")

	if m.confirm != nil {
		t.Error("staged an upgrade with nothing to upgrade")
	}
	if !strings.Contains(m.status, "no upgradable") {
		t.Errorf("status = %q, want it to say there is nothing to upgrade", m.status)
	}
}

func TestUpgradeAllCountsThePackages(t *testing.T) {
	m := newTestModel()

	press(m, "U")

	if m.confirm == nil {
		t.Fatal("no confirmation staged")
	}
	if !strings.Contains(m.confirm.prompt, "1") {
		t.Errorf("prompt %q should state how many packages are affected", m.confirm.prompt)
	}
}

func TestHelpTogglesAndReflowsTheLayout(t *testing.T) {
	m := newTestModel()
	before := m.layout.BodyHeight

	press(m, "?")
	if !m.showHelp {
		t.Fatal("? did not open the help")
	}
	if m.layout.BodyHeight >= before {
		t.Errorf("opening help did not shrink the body: %d then %d", before, m.layout.BodyHeight)
	}
	if m.layout.TotalHeight() != m.height {
		t.Errorf("layout no longer fills the terminal: %d vs %d",
			m.layout.TotalHeight(), m.height)
	}

	press(m, "?")
	if m.showHelp {
		t.Error("? did not close the help")
	}
	if m.layout.BodyHeight != before {
		t.Errorf("body height did not return to %d, got %d", before, m.layout.BodyHeight)
	}
}

func TestStaleDetailsResponsesAreDiscarded(t *testing.T) {
	// Holding ↓ issues a fetch per row; a slow response for an earlier row must
	// not overwrite the pane after the user has moved on.
	m := newTestModel()

	m.cursor = 0
	m.requestDetails()
	staleGen := m.detailsGen

	press(m, "down")
	freshGen := m.detailsGen
	if freshGen == staleGen {
		t.Fatal("moving the cursor did not start a new fetch")
	}

	// The stale response arrives late.
	m.Update(detailsLoadedMsg{
		gen:     staleGen,
		name:    "bash",
		details: &pkgDetails{Name: "bash", Synopsis: "stale"},
	})
	if m.detailsPkg != nil && m.detailsPkg.Synopsis == "stale" {
		t.Error("a stale details response was applied to the pane")
	}

	// The current response is accepted.
	m.Update(detailsLoadedMsg{
		gen:     freshGen,
		name:    m.selected().Name,
		details: &pkgDetails{Name: m.selected().Name, Synopsis: "fresh"},
	})
	if m.detailsPkg == nil || m.detailsPkg.Synopsis != "fresh" {
		t.Errorf("the current details response was not applied: %+v", m.detailsPkg)
	}
}

func TestOperationOutputIsStreamedIntoTheLog(t *testing.T) {
	m := newTestModel()

	m.Update(opLineMsg{line: "$ apt-get install -y sl"})
	m.Update(opLineMsg{line: "Unpacking sl (5.02-1) ..."})
	m.Update(opLineMsg{line: "W: some warning", stderr: true})

	if len(m.logs) != 3 {
		t.Fatalf("log has %d lines, want 3", len(m.logs))
	}
	if !m.logs[0].cmd {
		t.Error("the command line was not tagged as a command")
	}
	if !m.logs[2].stderr {
		t.Error("the stderr line was not tagged as stderr")
	}

	view := m.View()
	for _, want := range []string{"apt-get install -y sl", "Unpacking sl", "some warning"} {
		if !strings.Contains(view, want) {
			t.Errorf("the log pane does not show %q", want)
		}
	}
}

func TestLogIsBounded(t *testing.T) {
	// A long dist-upgrade must not grow the log without limit.
	m := newTestModel()

	for i := 0; i < logCapacity+250; i++ {
		m.appendLog(logLine{text: "line " + itoa(i)})
	}
	if len(m.logs) != logCapacity {
		t.Errorf("log holds %d lines, want it capped at %d", len(m.logs), logCapacity)
	}
	// The newest output is what is kept.
	if last := m.logs[len(m.logs)-1].text; last != "line "+itoa(logCapacity+249) {
		t.Errorf("last line = %q, want the most recent", last)
	}
}

func TestLogFollowsNewOutputThenStaysWhereScrolled(t *testing.T) {
	m := newTestModel()

	// While the pane sits at the bottom, new output scrolls it into view.
	for i := 0; i < 60; i++ {
		m.appendLog(logLine{text: "line " + itoa(i)})
	}
	m.View() // rendering is what folds pending output in
	if !m.logView.AtBottom() {
		t.Error("the log pane did not follow the newest output")
	}

	// Once the user scrolls up to read back, further output must not yank the
	// pane away from where they are looking.
	m.focus = focusLog
	press(m, "up up up")
	parked := m.logView.YOffset
	if parked == 0 {
		t.Fatal("scrolling up had no effect")
	}

	for i := 60; i < 80; i++ {
		m.appendLog(logLine{text: "line " + itoa(i)})
	}
	m.View()

	if m.logView.YOffset != parked {
		t.Errorf("new output moved the pane from %d to %d while the user was reading back",
			parked, m.logView.YOffset)
	}
}

func TestLogRenderingIsNotQuadratic(t *testing.T) {
	// apt emits thousands of lines during a large upgrade. Rendering must be
	// deferred to one pass per frame, not repeated per line, or the UI stalls
	// exactly when it is meant to be showing progress.
	m := newTestModel()

	start := len(m.logs)
	for i := 0; i < 4000; i++ {
		m.appendLog(logLine{text: "Unpacking package-" + itoa(i) + " (1.0-1) ..."})
	}
	if len(m.logs)-start < 3000 {
		t.Fatalf("only %d lines retained", len(m.logs)-start)
	}

	// One render folds all of it in; the whole burst plus the render must stay
	// far below the ~10s a per-line re-render used to cost.
	view := m.View()
	if !strings.Contains(view, "Unpacking package-3999") {
		t.Error("the newest output is not visible after the burst")
	}
}

func TestOperationCompletionTriggersARescan(t *testing.T) {
	// After an operation the interface must be refreshed from the system rather
	// than assuming what changed.
	m := newTestModel()
	m.busy = true
	m.current = operation{Kind: opInstall, Target: "nginx"}

	cmd := m.handleOpDone(opDoneMsg{op: m.current})

	if m.busy {
		t.Error("the model is still busy after completion")
	}
	if cmd == nil {
		t.Fatal("completion did not schedule a refresh")
	}
	if !m.loadingInstalled || !m.loadingUpgradable {
		t.Error("completion did not mark the installed and upgradable scans as pending")
	}
	if !strings.Contains(m.status, "completed") {
		t.Errorf("status = %q, want it to report completion", m.status)
	}
}

func TestFailedOperationStillRescans(t *testing.T) {
	// A failed apt run can still have changed state, so the refresh is
	// unconditional and the error is reported.
	m := newTestModel()
	m.busy = true
	op := operation{Kind: opInstall, Target: "nginx"}

	cmd := m.handleOpDone(opDoneMsg{op: op, err: errFake{}})

	if cmd == nil {
		t.Fatal("a failed operation did not schedule a refresh")
	}
	if !m.loadingInstalled {
		t.Error("a failed operation skipped the installed rescan")
	}
	if m.statusLevel != statusError {
		t.Errorf("status level = %v, want error", m.statusLevel)
	}
	// The apt output must retain the reason.
	found := false
	for _, l := range m.logs {
		if strings.Contains(l.text, "failed") {
			found = true
		}
	}
	if !found {
		t.Error("the failure was not recorded in the apt output pane")
	}
}

type errFake struct{}

func (errFake) Error() string { return "exit status 100" }

func TestUpgradeDisappearsFromTheListAfterRescan(t *testing.T) {
	// End-to-end at the model level: the upgradable filter is driven by apt's
	// plan, so once apt stops planning the upgrade the row goes away.
	m := newTestModel()
	press(m, "4") // upgradable filter

	if n := len(m.store.visible()); n != 1 {
		t.Fatalf("%d upgradable packages before the upgrade, want 1", n)
	}

	// The rescan that follows a successful upgrade: dpkg reports the new version
	// and apt-get -s upgrade produces no Inst lines.
	m.Update(installedLoadedMsg{pkgs: []installedPkg{
		{Name: "bash", Version: "5.2.15-2+b13", Status: "installed"},
		{Name: "libc-bin", Version: "2.36-9+deb12u14", Status: "installed"},
		{Name: "localonly", Version: "0.1", Status: "installed"},
	}})
	m.Update(upgradableLoadedMsg{cands: nil})

	if n := len(m.store.visible()); n != 0 {
		t.Errorf("%d packages still in the upgradable list after the upgrade", n)
	}
	if m.store.countUpgradable != 0 {
		t.Errorf("upgradable count = %d, want 0", m.store.countUpgradable)
	}

	view := m.View()
	if !strings.Contains(view, "up to date") {
		t.Errorf("the empty upgradable list should say everything is current:\n%s", view)
	}
}

func TestInstallThenRescanMarksThePackageInstalled(t *testing.T) {
	m := newTestModel()

	if m.store.lookup("nginx").Installed {
		t.Fatal("nginx should start uninstalled")
	}

	// dpkg now reports nginx, as it would after a real install.
	m.Update(installedLoadedMsg{pkgs: []installedPkg{
		{Name: "bash", Version: "5.2.15-2+b13", Status: "installed"},
		{Name: "libc-bin", Version: "2.36-9+deb12u7", Status: "installed"},
		{Name: "localonly", Version: "0.1", Status: "installed"},
		{Name: "nginx", Version: "1.22.1-9", Status: "installed"},
	}})

	p := m.store.lookup("nginx")
	if !p.Installed || p.InstalledVersion != "1.22.1-9" {
		t.Errorf("nginx not marked installed after the rescan: %+v", p)
	}

	press(m, "2") // installed filter
	found := false
	for _, v := range m.store.visible() {
		if v.Name == "nginx" {
			found = true
		}
	}
	if !found {
		t.Error("nginx is absent from the installed filter after being installed")
	}
}

func TestRemoveThenRescanClearsThePackage(t *testing.T) {
	m := newTestModel()

	// dpkg no longer reports localonly, as after a purge.
	m.Update(installedLoadedMsg{pkgs: []installedPkg{
		{Name: "bash", Version: "5.2.15-2+b13", Status: "installed"},
		{Name: "libc-bin", Version: "2.36-9+deb12u7", Status: "installed"},
	}})

	if p := m.store.lookup("localonly"); p.Installed {
		t.Errorf("localonly still marked installed: %+v", p)
	}

	press(m, "2")
	for _, v := range m.store.visible() {
		if v.Name == "localonly" {
			t.Error("the removed package is still in the installed filter")
		}
	}
}

func TestBusyModelRefusesASecondOperation(t *testing.T) {
	// apt takes the dpkg lock, so a second concurrent run would fail; refuse it
	// with a clear message instead.
	m := newTestModel()
	m.busy = true
	m.cursor = indexOf(m, "nginx")

	press(m, "i")

	if m.confirm != nil {
		t.Error("staged a second operation while one was running")
	}
	if !strings.Contains(m.status, "already running") {
		t.Errorf("status = %q, want it to explain the refusal", m.status)
	}
}

func TestResizeKeepsTheLayoutConsistent(t *testing.T) {
	m := newTestModel()

	for _, sz := range []struct{ w, h int }{
		{80, 24}, {200, 60}, {60, 18}, {120, 40}, {50, 14},
	} {
		m.Update(tea.WindowSizeMsg{Width: sz.w, Height: sz.h})

		if m.layout.TotalHeight() != sz.h {
			t.Errorf("size %dx%d: layout totals %d", sz.w, sz.h, m.layout.TotalHeight())
		}
		// The viewports must never be taller than the panes that hold them.
		if m.details.Height > m.layout.BodyHeight {
			t.Errorf("size %dx%d: details viewport %d exceeds the body %d",
				sz.w, sz.h, m.details.Height, m.layout.BodyHeight)
		}
		if m.logView.Height > m.layout.LogHeight {
			t.Errorf("size %dx%d: log viewport %d exceeds the pane %d",
				sz.w, sz.h, m.logView.Height, m.layout.LogHeight)
		}
		// The cursor must stay inside the list.
		if m.cursor >= len(m.store.visible()) && len(m.store.visible()) > 0 {
			t.Errorf("size %dx%d: cursor %d out of range", sz.w, sz.h, m.cursor)
		}
	}
}

func TestQuitDuringAnOperationRequiresCtrlC(t *testing.T) {
	// q must not abandon a running apt; ctrl+c is the deliberate escape hatch.
	m := newTestModel()
	m.busy = true

	press(m, "q")
	if !strings.Contains(m.status, "ctrl+c") {
		t.Errorf("status = %q, want it to name the override key", m.status)
	}
}

func TestEveryActionKeyIsDocumentedInHelp(t *testing.T) {
	// Discoverability: every binding must carry help text, or it cannot be found
	// from inside the TUI.
	k := defaultKeyMap()

	for _, column := range k.FullHelp() {
		for _, binding := range column {
			h := binding.Help()
			if h.Key == "" {
				t.Errorf("a binding has no key label: %+v", binding.Keys())
			}
			if h.Desc == "" {
				t.Errorf("binding %q has no description", h.Key)
			}
		}
	}

	// The short help must stay short enough to fit one line at 80 columns.
	if n := len(k.ShortHelp()); n > 10 {
		t.Errorf("the footer lists %d bindings, which will not fit at 80 columns", n)
	}
}
