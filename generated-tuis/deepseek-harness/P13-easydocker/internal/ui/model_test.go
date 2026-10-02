package ui

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"toolm/internal/docker"
)

func TestViewRenders(t *testing.T) {
	m := New(nil)
	nm, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	model := nm.(Model)
	if v := model.View(); v == "" {
		t.Fatal("expected non-empty view")
	}

	// Switching views and tabs should not panic.
	nm, _ = model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'3'}})
	model = nm.(Model)
	_ = model.View()
}

func TestQuitKey(t *testing.T) {
	m := New(nil)
	nm, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'q'}})
	_ = nm
	if cmd == nil {
		t.Fatal("expected a quit command")
	}
}

func TestListRenderWithData(t *testing.T) {
	m := New(nil)
	nm, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	model := nm.(Model)

	// Simulate a completed containers load.
	nm, _ = model.Update(containersMsg{items: []docker.Container{
		{ID: "abc123def456", Names: []string{"/web"}, Image: "nginx:latest", Status: "Up 3 hours"},
	}})
	model = nm.(Model)

	v := model.View()
	if !strings.Contains(v, "web") || !strings.Contains(v, "nginx:latest") {
		t.Fatalf("expected container name and image in rendered view:\n%s", v)
	}

	// Switch to images and simulate a load, checking size formatting on screen.
	nm, _ = model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})
	model = nm.(Model)
	nm, _ = model.Update(imagesMsg{items: []docker.Image{
		{ID: "sha256:img123", RepoTags: []string{"nginx:latest"}, Size: 150000000},
	}})
	model = nm.(Model)

	v = model.View()
	if !strings.Contains(v, "143.1 MB") || !strings.Contains(v, "img123") {
		t.Fatalf("expected formatted image size and short id in rendered view:\n%s", v)
	}
}

func TestDetailAndLogsNavigation(t *testing.T) {
	m := New(nil)
	nm, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	model := nm.(Model)
	nm, _ = model.Update(containersMsg{items: []docker.Container{
		{ID: "abc123def456", Names: []string{"/web"}, Image: "nginx:latest", Status: "Up 3 hours"},
	}})
	model = nm.(Model)

	// Open detail: should transition to viewDetail and issue a load command.
	nm, cmd := model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'l'}})
	model = nm.(Model)
	if cmd == nil {
		t.Fatal("expected a logs command")
	}
	if model.view != viewLogs {
		t.Fatalf("view = %v, want viewLogs", model.view)
	}

	// Esc should return to the list.
	nm, _ = model.Update(tea.KeyMsg{Type: tea.KeyEsc})
	model = nm.(Model)
	if model.view != viewList {
		t.Fatalf("view = %v, want viewList", model.view)
	}
}

func TestFormatSizeMB(t *testing.T) {
	cases := map[int64]string{
		0:          "0.0 MB",
		1048576:    "1.0 MB",
		150000000:  "143.1 MB",
		268435456:  "256.0 MB",
		1073741824: "1024.0 MB",
	}
	for in, want := range cases {
		if got := formatSizeMB(in); got != want {
			t.Errorf("formatSizeMB(%d) = %q, want %q", in, got, want)
		}
	}
}
