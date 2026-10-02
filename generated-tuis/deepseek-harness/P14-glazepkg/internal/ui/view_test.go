package ui

import (
	"testing"

	"github.com/tooln/tooln/internal/pkgs"
)

// TestViewDoesNotPanic exercises every rendering path (empty list, populated
// list, search, confirm, help) to catch nil-dereference / index panics.
func TestViewDoesNotPanic(t *testing.T) {
	m := New()
	m.width, m.height, m.ready = 80, 24, true

	// Empty list.
	m.packages = nil
	m.applyFilter("")
	if got := m.View(); got == "" {
		t.Fatal("empty-list view rendered nothing")
	}

	// Populated list with details.
	m.packages = []pkgs.Package{
		{Name: "numpy", Version: "1.26.0", Manager: "pip"},
		{Name: "requests", Version: "2.31.0", Manager: "pip"},
	}
	m.applyFilter("")
	m.details = &pkgs.PackageInfo{Name: "requests", Version: "2.31.0", Depends: []string{"certifi", "idna"}}
	if got := m.View(); got == "" {
		t.Fatal("browse view rendered nothing")
	}

	// Filter mode.
	m.enterFilter()
	if got := m.View(); got == "" {
		t.Fatal("filter view rendered nothing")
	}

	// Search mode with results.
	m.enterSearch()
	m.searchInput.SetValue("req")
	m.searchResults = []pkgs.SearchResult{{Name: "requests", Version: "2.31.0", Summary: "HTTP for Humans."}}
	if got := m.View(); got == "" {
		t.Fatal("search view rendered nothing")
	}

	// Confirm mode.
	m.mode = modeConfirm
	m.confirmAction = "uninstall"
	m.confirmTarget = "requests"
	if got := m.View(); got == "" {
		t.Fatal("confirm view rendered nothing")
	}

	// Help mode.
	m.enterHelp()
	if got := m.View(); got == "" {
		t.Fatal("help view rendered nothing")
	}
}

// TestLayoutSanity ensures the layout math stays within sane bounds for small
// and large terminals.
func TestLayoutSanity(t *testing.T) {
	m := New()
	for _, dim := range [][2]int{{10, 5}, {40, 12}, {80, 24}, {200, 50}} {
		m.width, m.height, m.ready = dim[0], dim[1], true
		lw, dw, lr, dr := m.layout()
		if lw < 10 || dw < 10 || lr < 0 || dr < 0 {
			t.Fatalf("layout(%dx%d) = listW=%d detailsW=%d listRows=%d detailsRows=%d", dim[0], dim[1], lw, dw, lr, dr)
		}
		_ = m.View()
	}
}
