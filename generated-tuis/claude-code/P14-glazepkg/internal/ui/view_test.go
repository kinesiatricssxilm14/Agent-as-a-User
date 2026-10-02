package ui

import (
	"context"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"tooln/internal/pkgmgr"
)

// fakeManager is a Manager that answers from fixed data. It lets the view be
// exercised at many terminal sizes without touching the real pip or apt.
type fakeManager struct {
	id    string
	pkgs  []pkgmgr.Package
	fails bool
}

func (f *fakeManager) ID() string    { return f.id }
func (f *fakeManager) Label() string { return f.id }
func (f *fakeManager) Kind() string  { return f.id + " packages" }

func (f *fakeManager) Probe(context.Context) error {
	if f.fails {
		return context.Canceled
	}
	return nil
}

func (f *fakeManager) List(context.Context) ([]pkgmgr.Package, error) { return f.pkgs, nil }

func (f *fakeManager) Search(_ context.Context, q string) ([]pkgmgr.Package, error) {
	return f.pkgs, nil
}

func (f *fakeManager) Details(_ context.Context, p pkgmgr.Package) (*pkgmgr.Details, error) {
	return &pkgmgr.Details{
		Name:    p.Name,
		Version: p.Version,
		Fields: []pkgmgr.Field{
			{Key: "Name", Value: p.Name},
			{Key: "Summary", Value: strings.Repeat("a long summary that wraps ", 12)},
		},
		Requires: []string{"dep-one", "dep-two"},
	}, nil
}

func (f *fakeManager) Outdated(context.Context) (map[string]string, error) {
	return map[string]string{"alpha": "9.9.9"}, nil
}

func (f *fakeManager) InstallPlan(_ context.Context, spec string) (pkgmgr.Plan, error) {
	return pkgmgr.Plan{Title: "install " + spec,
		Steps: []pkgmgr.Command{{Name: "true", Args: []string{spec}}}}, nil
}

func (f *fakeManager) UpgradePlan(_ context.Context, n string) (pkgmgr.Plan, error) {
	return pkgmgr.Plan{Title: "upgrade " + n,
		Steps: []pkgmgr.Command{{Name: "true", Args: []string{n}}}}, nil
}

func (f *fakeManager) RemovePlan(_ context.Context, n string) (pkgmgr.Plan, error) {
	return pkgmgr.Plan{Title: "remove " + n,
		Steps: []pkgmgr.Command{{Name: "true", Args: []string{n}}}}, nil
}

func (f *fakeManager) Extras() []pkgmgr.Extra {
	return []pkgmgr.Extra{
		{Key: "u", Title: "an update action", Plan: pkgmgr.Plan{
			Title: "update", Steps: []pkgmgr.Command{{Name: "true"}}}},
		{Key: "c", Title: "a cleanup action", Plan: pkgmgr.Plan{
			Title: "clean", Steps: []pkgmgr.Command{{Name: "true"}}}},
	}
}

func (f *fakeManager) SpecHint() string { return "a package name" }

func samplePackages(n int) []pkgmgr.Package {
	names := []string{"alpha", "beta-tools", "gamma_lib", "delta.io", "epsilon"}
	out := make([]pkgmgr.Package, 0, n)
	for i := 0; i < n; i++ {
		out = append(out, pkgmgr.Package{
			Name:      names[i%len(names)] + "-" + string(rune('a'+i%26)),
			Version:   "1.2.3",
			Summary:   "a package summary long enough to need truncating in narrow terminals",
			Installed: true,
		})
	}
	return out
}

// newTestModel returns a model wired to fake managers and already sized.
func newTestModel(t *testing.T, w, h, pkgCount int) *Model {
	t.Helper()
	m := New()
	m.SetManagers(
		&fakeManager{id: "pip", pkgs: samplePackages(pkgCount)},
		&fakeManager{id: "apt", pkgs: samplePackages(pkgCount)},
	)
	// Drive the startup sequence the way the runtime would, but synchronously.
	m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	for _, tab := range m.tabs {
		m.Update(probeDoneMsg{mgr: tab.mgr.ID()})
		m.Update(listMsg{mgr: tab.mgr.ID(), pkgs: tab.mgr.(*fakeManager).pkgs})
	}
	return m
}

// viewHeight is the number of terminal rows the rendered view occupies.
func viewHeight(v string) int { return len(strings.Split(v, "\n")) }

func viewWidth(v string) int {
	w := 0
	for _, line := range strings.Split(v, "\n") {
		if n := lipglossWidth(line); n > w {
			w = n
		}
	}
	return w
}

// TestViewFitsTerminal is the regression guard for the layout: the rendered view
// must occupy exactly the terminal height in every mode and at every size.
// lipgloss's Height() is a minimum rather than a maximum, so an over-tall panel
// silently scrolls the header off the screen unless the output is clipped.
func TestViewFitsTerminal(t *testing.T) {
	sizes := []struct{ w, h int }{
		{80, 24}, {100, 30}, {200, 50}, {40, 12}, {60, 15}, {250, 80}, {45, 13},
	}
	// Each step is a key sequence that puts the model into a distinct mode.
	modes := []struct {
		name string
		keys []string
	}{
		{"browse", nil},
		{"filter", []string{"f"}},
		{"filter-typed", []string{"f", "a"}},
		{"search-prompt", []string{"s"}},
		{"install-prompt", []string{"i"}},
		{"confirm-remove", []string{"d"}},
		{"confirm-upgrade", []string{"U"}},
		{"menu", []string{"m"}},
		{"help", []string{"?"}},
		{"log", []string{"L"}},
		{"log+help", []string{"L", "?"}},
		{"log+menu", []string{"L", "m"}},
		{"log+confirm", []string{"L", "d"}},
		{"apt-tab", []string{"tab"}},
		{"apt-tab-menu", []string{"tab", "m"}},
		{"marks", []string{" ", " ", " "}},
		{"marks-confirm", []string{" ", " ", "d"}},
		{"details-focus", []string{"right"}},
		{"empty-filter", []string{"f", "z", "z", "z", "q", "x"}},
	}

	for _, size := range sizes {
		for _, mode := range modes {
			for _, count := range []int{0, 1, 3, 60} {
				m := newTestModel(t, size.w, size.h, count)
				for _, k := range mode.keys {
					m.Update(keyMsgFor(k))
				}
				v := m.View()
				if got := viewHeight(v); got != size.h {
					t.Errorf("%dx%d mode=%s pkgs=%d: view is %d rows, want %d",
						size.w, size.h, mode.name, count, got, size.h)
				}
				if got := viewWidth(v); got > size.w {
					t.Errorf("%dx%d mode=%s pkgs=%d: view is %d columns wide, want <= %d",
						size.w, size.h, mode.name, count, got, size.w)
				}
			}
		}
	}
}

// TestHeaderAlwaysVisible checks that the tab bar survives every mode: it is how
// the user discovers that two managers exist, so it must never be pushed off.
func TestHeaderAlwaysVisible(t *testing.T) {
	for _, keys := range [][]string{
		nil, {"?"}, {"m"}, {"d"}, {"L", "m"}, {"L", "d"}, {"i"}, {"f"}, {"tab", "m"},
	} {
		m := newTestModel(t, 100, 24, 40)
		for _, k := range keys {
			m.Update(keyMsgFor(k))
		}
		first := strings.Split(m.View(), "\n")
		if !strings.Contains(first[0], "tooln") {
			t.Errorf("keys=%v: first row is %q, want the title", keys, first[0])
		}
		if len(first) < 2 || !strings.Contains(first[1], "pip") || !strings.Contains(first[1], "apt") {
			t.Errorf("keys=%v: second row is %q, want both manager tabs", keys, first[1])
		}
	}
}

// TestShortHelpAlwaysPresent checks the key reminder is on screen in every mode,
// which is what makes the interface explorable without external documentation.
func TestShortHelpAlwaysPresent(t *testing.T) {
	cases := []struct {
		keys []string
		want string
	}{
		{nil, "q quit"},
		{[]string{"f"}, "esc cancel"},
		{[]string{"s"}, "esc cancel"},
		{[]string{"i"}, "esc cancel"},
		{[]string{"d"}, "esc/n"},
		{[]string{"m"}, "esc close"},
		{[]string{"?"}, "close help"},
	}
	for _, tc := range cases {
		m := newTestModel(t, 120, 30, 20)
		for _, k := range tc.keys {
			m.Update(keyMsgFor(k))
		}
		lines := strings.Split(m.View(), "\n")
		last := lines[len(lines)-1]
		if !strings.Contains(last, tc.want) {
			t.Errorf("keys=%v: last row %q does not mention %q", tc.keys, last, tc.want)
		}
	}
}
