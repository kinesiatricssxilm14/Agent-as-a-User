package main

import (
	"reflect"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func withData() *model {
	m := initialModel()
	m.width, m.height, m.ready = 120, 32, true
	pkgs := samplePackages()
	m.allPackages = pkgs
	m.pkgByName = map[string]Package{}
	for _, p := range pkgs {
		m.pkgByName[p.Name] = p
	}
	m.availableMap = map[string]Package{"zip": pkgs[2]}
	m.installedMap = map[string]Package{"apache2": pkgs[0], "openssl": pkgs[1]}
	m.upgradable = map[string]Package{"openssl": pkgs[1]}
	m.loading = false
	m.rebuildList()
	return &m
}

func isQuit(cmd tea.Cmd) bool {
	return cmd != nil && reflect.ValueOf(cmd).Pointer() == reflect.ValueOf(tea.Quit).Pointer()
}

func TestQWhileSearchingTypesNotQuit(t *testing.T) {
	m := withData()
	m.focus = FocusSearch
	m.searchInput.Focus()

	nm, cmd := m.handleKey(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'q'}})
	if isQuit(cmd) {
		t.Fatal("q while searching should not quit")
	}
	got := nm.(*model)
	if got.searchInput.Value() != "q" {
		t.Errorf("expected search value %q, got %q", "q", got.searchInput.Value())
	}
	if got.state != StateBrowse {
		t.Errorf("expected browse state, got %v", got.state)
	}
}

func TestQWhileListFocusedQuits(t *testing.T) {
	m := withData()
	m.focus = FocusList

	_, cmd := m.handleKey(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'q'}})
	if !isQuit(cmd) {
		t.Error("q while list focused should quit")
	}
}

func TestModeSwitch(t *testing.T) {
	m := withData()
	m.focus = FocusList

	nm, _ := m.handleKey(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'3'}})
	got := nm.(*model)
	if got.mode != ModeUpgradable {
		t.Errorf("expected upgradable mode, got %v", got.mode)
	}
	if len(got.filtered) != 1 || got.filtered[0].Name != "openssl" {
		t.Errorf("upgradable view should contain only openssl, got %v", got.filtered)
	}
}

func TestPrepareInstallConfirm(t *testing.T) {
	m := withData()
	m.list.Select(2) // zip (available)

	cmd := m.prepareAction(Action{Kind: "install"})
	if cmd != nil {
		t.Fatal("prepareAction should not return a command")
	}
	if m.state != StateConfirm {
		t.Fatalf("expected confirm state, got %v", m.state)
	}
	if m.pendingAction.Target != "zip" {
		t.Errorf("expected target zip, got %q", m.pendingAction.Target)
	}
}

func TestPrepareRemoveOnInstalledRejected(t *testing.T) {
	m := withData()
	m.list.Select(0) // apache2 (installed)

	m.prepareAction(Action{Kind: "install"})
	if m.state == StateConfirm {
		t.Error("installing an already-installed package should not ask for confirmation")
	}
}
