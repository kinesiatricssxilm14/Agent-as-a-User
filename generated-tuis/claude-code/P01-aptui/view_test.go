package main

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// These tests drive the real View() and assert the properties the spec demands
// of a screen snapshot: the whole interface fits the terminal, and the list, the
// details (with every dependency on its own line), the apt log, the status line
// and the key help are all present at once.

// renderModel builds a model populated from fixture data and renders it at the
// given size, returning the screen as lines.
func renderModel(t *testing.T, width, height int, setup func(*model)) []string {
	t.Helper()

	// Force a fixed colour profile so the rendered width does not depend on
	// whatever terminal the test happens to run under.
	lipgloss.SetColorProfile(0) // Ascii: no escape sequences

	m := newModel()
	m.store = testStore()
	m.store.rebuild()

	// testStore represents the state after the initial scans have completed, so
	// clear the loading flags to match; otherwise the view correctly reports that
	// it is still loading.
	m.loadingInstalled = false
	m.loadingAvailable = false
	m.loadingUpgradable = false
	m.aptListsChecked = true
	m.aptListsPresent = true
	// newModel warns when apt is absent, which it is on the test host; that
	// warning is not what these tests are about.
	m.startupWarning = ""
	m.status = ""

	if setup != nil {
		setup(m)
	}

	m.Update(tea.WindowSizeMsg{Width: width, Height: height})

	view := m.View()
	return strings.Split(view, "\n")
}

// withDetails attaches a parsed control record so the details pane has content,
// exactly as it would after a real apt-cache show.
func withDetails(m *model) {
	m.cursor = 0
	p := m.selected()
	if p == nil {
		return
	}
	det := detailsFromRecord(parseControl(bashShowFixture))
	det.Origin = "apt-cache show"

	m.detailsFor = p.Name
	m.detailsPkg = det
	m.detailsLoading = false
}

func TestViewFitsTheTerminalExactly(t *testing.T) {
	sizes := []struct{ w, h int }{
		{80, 24}, {120, 40}, {100, 30}, {200, 50}, {60, 20}, {50, 14},
	}

	for _, sz := range sizes {
		for _, help := range []bool{false, true} {
			lines := renderModel(t, sz.w, sz.h, func(m *model) {
				withDetails(m)
				m.showHelp = help
				m.help.ShowAll = help
			})

			if len(lines) > sz.h {
				t.Errorf("size %dx%d help=%v: rendered %d lines, terminal has %d",
					sz.w, sz.h, help, len(lines), sz.h)
			}
			for i, line := range lines {
				if w := lipgloss.Width(line); w > sz.w {
					t.Errorf("size %dx%d help=%v: line %d is %d cells wide: %q",
						sz.w, sz.h, help, i, w, line)
				}
			}
		}
	}
}

func TestViewShowsEveryPaneOnOneScreen(t *testing.T) {
	// The core same-screen requirement: one snapshot contains the header counts,
	// the search line, the package list, the details pane, the apt output pane,
	// the status line and the key help. Nothing requires a tab or a page turn.
	lines := renderModel(t, 120, 40, func(m *model) {
		withDetails(m)
		m.appendLog(logLine{text: "$ apt-get install -y sl", cmd: true})
		m.appendLog(logLine{text: "Setting up sl (5.02-1) ..."})
	})
	screen := strings.Join(lines, "\n")

	wants := []struct {
		what     string
		fragment string
	}{
		{"tool name", "toola"},
		{"installed count", "installed"},
		{"upgradable count", "upgradable"},
		{"search line", "Search:"},
		{"list column header", "PACKAGE"},
		{"a package from the list", "bash"},
		{"another package from the list", "nginx"},
		{"details description heading", "Description"},
		{"details dependency heading", "Depends"},
		{"apt output pane", "apt output"},
		{"streamed apt command", "apt-get install -y sl"},
		{"streamed apt output", "Setting up sl"},
		{"key help: install", "install"},
		{"key help: search", "search"},
		{"key help: quit", "quit"},
	}

	for _, w := range wants {
		if !strings.Contains(screen, w.fragment) {
			t.Errorf("the screen does not show the %s (%q missing)\n---\n%s",
				w.what, w.fragment, screen)
		}
	}
}

func TestViewListsEveryDependencyOnItsOwnLine(t *testing.T) {
	// The spec requires all dependency package names listed one by one; a
	// comma-joined summary is explicitly insufficient. bashShowFixture has
	// Depends: base-files, debianutils and Pre-Depends: libc6, libtinfo6.
	lines := renderModel(t, 130, 46, withDetails)

	for _, dep := range []string{"base-files", "debianutils", "libc6", "libtinfo6"} {
		found := false
		for _, line := range lines {
			if strings.Contains(line, dep) {
				found = true
				// The dependency must be alone on its line, not joined to the
				// next one by a comma.
				if strings.Count(line, ",") > 0 {
					t.Errorf("dependency %q shares its line with others: %q", dep, line)
				}
				break
			}
		}
		if !found {
			t.Errorf("dependency %q is not shown on the screen:\n%s",
				dep, strings.Join(lines, "\n"))
		}
	}
}

func TestViewDetailsPaneShowsRelationCounts(t *testing.T) {
	lines := renderModel(t, 130, 46, withDetails)
	screen := strings.Join(lines, "\n")

	// Each relation group is headed with its count, so the user can tell at a
	// glance whether the pane needs scrolling to show the rest.
	for _, want := range []string{"Pre-Depends (2)", "Depends (2)", "Recommends (1)"} {
		if !strings.Contains(screen, want) {
			t.Errorf("missing relation heading %q:\n%s", want, screen)
		}
	}
}

func TestViewConfirmationIsInlineNotAnOverlay(t *testing.T) {
	// A confirmation must not hide the list or the details: overlays are
	// prohibited, so the prompt lives in the status line.
	lines := renderModel(t, 120, 40, func(m *model) {
		withDetails(m)
		m.confirm = &pendingConfirm{
			op:      operation{Kind: opInstall, Target: "nginx"},
			prompt:  "Install nginx (1.22.1-9)?",
			warning: "",
		}
	})
	screen := strings.Join(lines, "\n")

	if !strings.Contains(screen, "Install nginx") {
		t.Errorf("the confirmation prompt is not shown:\n%s", screen)
	}
	if !strings.Contains(screen, "[y/n]") {
		t.Errorf("the confirmation does not show its keys:\n%s", screen)
	}
	// Everything else must still be visible behind the prompt.
	for _, fragment := range []string{"PACKAGE", "bash", "Description", "apt output"} {
		if !strings.Contains(screen, fragment) {
			t.Errorf("the confirmation hid %q from the screen:\n%s", fragment, screen)
		}
	}
}

func TestViewExpandedHelpDocumentsEveryBinding(t *testing.T) {
	// Shortcuts must be discoverable inside the TUI without external docs, so
	// the expanded help has to name every action.
	lines := renderModel(t, 160, 46, func(m *model) {
		withDetails(m)
		m.showHelp = true
		m.help.ShowAll = true
	})
	screen := strings.Join(lines, "\n")

	for _, want := range []string{
		"install", "remove", "purge", "upgrade selected", "upgrade all",
		"search", "cycle filter", "installed only", "upgradable only",
		"rescan system", "apt-get update", "confirm", "cancel", "quit",
		"page up", "page down",
	} {
		if !strings.Contains(screen, want) {
			t.Errorf("the help does not document %q:\n%s", want, screen)
		}
	}
}

func TestViewExpandedHelpDoesNotCoverTheList(t *testing.T) {
	// The help expands the footer and shrinks the body; it must not overlay it.
	lines := renderModel(t, 160, 46, func(m *model) {
		withDetails(m)
		m.showHelp = true
		m.help.ShowAll = true
	})
	screen := strings.Join(lines, "\n")

	for _, fragment := range []string{"PACKAGE", "bash", "Description"} {
		if !strings.Contains(screen, fragment) {
			t.Errorf("expanded help hid %q from the screen:\n%s", fragment, screen)
		}
	}
}

func TestViewEmptySearchResultExplainsItself(t *testing.T) {
	lines := renderModel(t, 100, 30, func(m *model) {
		m.search.SetValue("zzz-no-such-package")
		m.applyQuery("zzz-no-such-package")
	})
	screen := strings.Join(lines, "\n")

	if !strings.Contains(screen, "no package matches") {
		t.Errorf("an empty result set should say why:\n%s", screen)
	}
	// The way out must be stated, not left to be guessed.
	if !strings.Contains(screen, "esc") {
		t.Errorf("the empty-result message should name the key that clears it:\n%s", screen)
	}
}

func TestViewUpgradableFilterShowsTheUpgradeTarget(t *testing.T) {
	lines := renderModel(t, 120, 30, func(m *model) {
		m.store.filter = filterUpgradable
		m.store.rebuild()
		m.clampCursor()
	})
	screen := strings.Join(lines, "\n")

	if !strings.Contains(screen, "libc-bin") {
		t.Errorf("the upgradable package is not listed:\n%s", screen)
	}
	// The version being upgraded *to* is the number the decision turns on.
	if !strings.Contains(screen, "2.36-9+deb12u14") {
		t.Errorf("the upgrade target version is not shown:\n%s", screen)
	}
}

func TestViewBeforeFirstResizeDoesNotPanic(t *testing.T) {
	// Bubble Tea calls View once before the first WindowSizeMsg.
	m := newModel()
	if got := m.View(); got == "" {
		t.Error("View before the first resize returned nothing")
	}
}

func TestViewTinyTerminalWarnsInsteadOfBreaking(t *testing.T) {
	lines := renderModel(t, 20, 6, nil)
	screen := strings.Join(lines, "\n")

	if !strings.Contains(screen, "too small") {
		t.Errorf("a tiny terminal should explain the problem:\n%s", screen)
	}
	if len(lines) > 6 {
		t.Errorf("the warning overflowed the terminal: %d lines", len(lines))
	}
}

func TestViewNoPackagesSelectedIsHandled(t *testing.T) {
	// An empty store must not crash the details pane.
	m := newModel()
	m.Update(tea.WindowSizeMsg{Width: 100, Height: 30})

	view := m.View()
	if view == "" {
		t.Error("rendering an empty store produced nothing")
	}
	if !strings.Contains(view, "toola") {
		t.Errorf("the header is missing from an empty view:\n%s", view)
	}
}

func TestViewStateMarksDistinguishPackageStates(t *testing.T) {
	// The list encodes state in a leading glyph; each state must be distinct or
	// the column conveys nothing.
	m := newModel()
	m.store = testStore()
	m.store.rebuild()

	seen := map[string]string{}
	for _, name := range []string{"bash", "libc-bin", "nginx", "removedpkg"} {
		p := m.store.lookup(name)
		if p == nil {
			t.Fatalf("fixture is missing %q", name)
		}
		mark, _ := m.stateMark(p)
		if other, dup := seen[mark]; dup {
			t.Errorf("%q and %q share the mark %q", name, other, mark)
		}
		seen[mark] = name
	}
	if len(seen) != 4 {
		t.Errorf("expected 4 distinct marks, got %d: %v", len(seen), seen)
	}
}
