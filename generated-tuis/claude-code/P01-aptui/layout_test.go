package main

import "testing"

// The layout's contract is that every pane gets a non-overlapping slice of the
// terminal and the total is exactly the terminal height. That is what makes the
// "same-screen visibility" requirement hold: no pane can push another off-screen,
// so list, details, log, status and footer are always all present.

func TestComputeLayoutFillsExactlyTheTerminal(t *testing.T) {
	sizes := []struct{ w, h int }{
		{80, 24},  // the classic default
		{120, 40}, // a comfortable modern window
		{200, 60}, // wide
		{50, 14},  // the documented minimum
		{60, 20},
		{100, 30},
		{80, 100}, // very tall
		{300, 24}, // very wide
	}

	for _, footer := range []int{1, 6} { // collapsed and expanded help
		for _, busy := range []bool{false, true} {
			for _, sz := range sizes {
				l := computeLayout(sz.w, sz.h, footer, busy)

				if got := l.TotalHeight(); got != sz.h {
					t.Errorf("size %dx%d footer=%d busy=%v: total height %d, want %d (%+v)",
						sz.w, sz.h, footer, busy, got, sz.h, l)
				}
				if l.BodyHeight < 1 {
					t.Errorf("size %dx%d: body height %d must be positive", sz.w, sz.h, l.BodyHeight)
				}
				if l.ListWidth < 1 {
					t.Errorf("size %dx%d: list width %d must be positive", sz.w, sz.h, l.ListWidth)
				}
			}
		}
	}
}

func TestComputeLayoutWidthsFitTheTerminal(t *testing.T) {
	for w := minWidth; w <= 200; w++ {
		l := computeLayout(w, 30, 1, false)

		// The two panes plus their borders must be exactly the terminal width,
		// or JoinHorizontal would wrap and break the single-screen layout.
		total := l.ListWidth + l.DetailsWidth + 2*paneFrameWidth
		if total != w {
			t.Fatalf("width %d: list %d + details %d + frames %d = %d, want %d",
				w, l.ListWidth, l.DetailsWidth, 2*paneFrameWidth, total, w)
		}
	}
}

func TestComputeLayoutBothPanesAlwaysVisible(t *testing.T) {
	// The details pane must never collapse to nothing: the spec requires the
	// description and dependencies to be on the same screen as the list.
	for w := minWidth; w <= 200; w += 7 {
		l := computeLayout(w, 30, 1, false)
		if l.DetailsWidth < 1 {
			t.Errorf("width %d: details pane collapsed", w)
		}
		if l.ListWidth < minNameWidth {
			t.Errorf("width %d: list pane %d too narrow to show a package name", w, l.ListWidth)
		}
	}
}

func TestComputeLayoutLogGrowsWhileBusyButNeverDominates(t *testing.T) {
	idle := computeLayout(120, 40, 1, false)
	busy := computeLayout(120, 40, 1, true)

	if busy.LogHeight <= idle.LogHeight {
		t.Errorf("log pane should grow during an operation: idle %d, busy %d",
			idle.LogHeight, busy.LogHeight)
	}
	if busy.BodyHeight >= idle.BodyHeight {
		t.Errorf("the body should shrink to make room: idle %d, busy %d",
			idle.BodyHeight, busy.BodyHeight)
	}
	// The package list must keep the majority of the screen even mid-operation.
	if busy.LogHeight > busy.BodyHeight {
		t.Errorf("log pane %d should not exceed the body %d", busy.LogHeight, busy.BodyHeight)
	}
}

func TestComputeLayoutShortTerminalPrefersTheList(t *testing.T) {
	// When space runs out, height is taken from the log rather than the list:
	// seeing packages matters more than seeing old command output. At 80x40 the
	// log gets its full idle height; by 80x14 the body has been held at its
	// minimum and the log has given up rows to do it.
	tall := computeLayout(80, 40, 1, false)
	short := computeLayout(80, 14, 1, false)

	if short.LogHeight >= tall.LogHeight {
		t.Errorf("log pane should shrink first: tall %d, short %d",
			tall.LogHeight, short.LogHeight)
	}
	if short.BodyHeight < minBodyHeight {
		t.Errorf("body height %d fell below the minimum %d", short.BodyHeight, minBodyHeight)
	}
	if short.TotalHeight() != 14 {
		t.Errorf("total height %d, want 14", short.TotalHeight())
	}
}

func TestComputeLayoutBodyHoldsItsMinimumAsHeightFalls(t *testing.T) {
	// Across every height the tool declares usable, the list keeps at least
	// minBodyHeight rows and the log keeps at least minLogHeight, so neither
	// pane can be squeezed out of the single screen.
	for h := minHeight; h <= 60; h++ {
		l := computeLayout(80, h, 1, false)

		if l.BodyHeight < minBodyHeight {
			t.Errorf("height %d: body %d below minimum %d", h, l.BodyHeight, minBodyHeight)
		}
		if l.LogHeight < minLogHeight {
			t.Errorf("height %d: log %d below minimum %d", h, l.LogHeight, minLogHeight)
		}
		if l.TotalHeight() != h {
			t.Errorf("height %d: total %d", h, l.TotalHeight())
		}
	}
}

func TestComputeLayoutExpandedHelpShrinksTheBody(t *testing.T) {
	// The help view expands the footer in place rather than overlaying the
	// interface, so the body must give up the rows the footer takes.
	collapsed := computeLayout(100, 30, 1, false)
	expanded := computeLayout(100, 30, 7, false)

	if expanded.BodyHeight >= collapsed.BodyHeight {
		t.Errorf("expanding help should shrink the body: %d then %d",
			collapsed.BodyHeight, expanded.BodyHeight)
	}
	if expanded.TotalHeight() != 30 {
		t.Errorf("total height with expanded help = %d, want 30", expanded.TotalHeight())
	}
}

func TestComputeLayoutDegradedFlag(t *testing.T) {
	if l := computeLayout(120, 40, 1, false); l.Degraded {
		t.Error("a 120x40 terminal should not be degraded")
	}
	if l := computeLayout(30, 10, 1, false); !l.Degraded {
		t.Error("a 30x10 terminal should be flagged degraded")
	}
}

func TestComputeLayoutTinyTerminalDoesNotPanic(t *testing.T) {
	// Terminals report absurd sizes during resize; the arithmetic must survive.
	for _, sz := range []struct{ w, h int }{
		{0, 0}, {1, 1}, {5, 3}, {1, 100}, {100, 1}, {2, 2},
	} {
		l := computeLayout(sz.w, sz.h, 1, false)
		if l.BodyHeight < 1 || l.ListWidth < 1 {
			t.Errorf("size %dx%d produced unusable geometry %+v", sz.w, sz.h, l)
		}
	}
}

func TestSplitWidthNarrowFavoursTheList(t *testing.T) {
	// Below the point where both minimums fit, the list keeps its width because
	// without it there is nothing to select and nothing to show details for.
	list, details := splitWidth(minWidth)
	if list < minListWidth {
		t.Errorf("list width %d below the minimum %d", list, minListWidth)
	}
	if details < 1 {
		t.Errorf("details width %d must stay positive", details)
	}
}

func TestComputeListColumnsNeverExceedWidth(t *testing.T) {
	for w := 1; w <= 200; w++ {
		c := computeListColumns(w)

		// Mirror joinColumns: zero-width columns are dropped, and a single gap
		// sits between each pair that remains.
		widths := []int{c.Mark, c.Name, c.Version, c.Section, c.Synopsis}
		total, populated := 0, 0
		for _, cw := range widths {
			if cw <= 0 {
				continue
			}
			total += cw
			populated++
		}
		if populated > 1 {
			total += (populated - 1) * colGap
		}

		if total > w {
			t.Errorf("width %d: columns total %d, which overflows (%+v)", w, total, c)
		}
		if c.Name < 1 {
			t.Errorf("width %d: name column must always be present", w)
		}
	}
}

func TestComputeListColumnsDropsColumnsGracefully(t *testing.T) {
	// A narrow list keeps the name and sheds decoration; a wide one shows
	// everything, so the synopsis is visible without scrolling horizontally.
	narrow := computeListColumns(20)
	if narrow.Synopsis != 0 {
		t.Errorf("a 20-column list should not reserve a synopsis column: %+v", narrow)
	}

	wide := computeListColumns(120)
	if wide.Version == 0 || wide.Section == 0 || wide.Synopsis == 0 {
		t.Errorf("a 120-column list should show every column: %+v", wide)
	}
	if wide.Name > maxNameWidth {
		t.Errorf("name column %d exceeds the cap %d", wide.Name, maxNameWidth)
	}
}
