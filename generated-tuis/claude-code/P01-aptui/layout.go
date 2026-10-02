package main

// Layout arithmetic, kept pure so the pane geometry can be tested without a
// terminal. The contract this file enforces is the spec's "same-screen
// visibility" rule: every pane has a fixed, non-overlapping slice of the
// terminal, the total is exactly the terminal height, and nothing is ever drawn
// on top of anything else.

// layout holds the resolved geometry for one terminal size.
type layout struct {
	Width  int
	Height int

	// Fixed-height chrome.
	HeaderHeight int
	SearchHeight int
	StatusHeight int
	FooterHeight int

	// The middle row: list and details side by side, same height.
	BodyHeight   int
	ListWidth    int
	DetailsWidth int

	// The apt output pane below the body.
	LogHeight int

	// Degraded is set when the terminal is too small to honour the intended
	// minimums, so the view can warn instead of rendering something unusable.
	Degraded bool
}

// Layout constants. These are minimums and targets, not hard sizes: computeLayout
// gives the body whatever is left after the chrome, and only shrinks the log pane
// when there is nothing else to give.
const (
	headerHeight = 1 // title + counts
	searchHeight = 1 // search input
	statusHeight = 1 // status / confirmation prompt

	// The log pane is small while idle and grows during an operation, so apt's
	// real output is prominent exactly when it matters.
	logHeightIdle = 4
	logHeightBusy = 10

	// Below these the layout is declared degraded.
	minBodyHeight = 4
	minLogHeight  = 2
	minWidth      = 50
	minHeight     = 14

	// Border and padding overhead of the bordered panes, per axis.
	paneFrameHeight = 2 // top + bottom border
	paneFrameWidth  = 2 // left + right border

	// The list gets the larger share: names plus versions need more room than
	// the details pane, which wraps.
	listWidthPercent = 52
	minListWidth     = 24
	minDetailsWidth  = 22
)

// computeLayout resolves the geometry for a terminal of the given size.
//
// footerHeight is the measured height of the rendered footer, which varies
// because the help view expands in place; passing it in keeps this function pure
// and lets the caller use the real rendered height rather than guessing.
//
// busy indicates an operation is running, which grows the log pane.
func computeLayout(width, height, footerHeight int, busy bool) layout {
	l := layout{
		Width:        width,
		Height:       height,
		HeaderHeight: headerHeight,
		SearchHeight: searchHeight,
		StatusHeight: statusHeight,
		FooterHeight: footerHeight,
	}

	if width < minWidth || height < minHeight {
		l.Degraded = true
	}

	wantLog := logHeightIdle
	if busy {
		wantLog = logHeightBusy
	}

	// Vertical budget: chrome is fixed, the rest is split between body and log.
	chrome := l.HeaderHeight + l.SearchHeight + l.StatusHeight + l.FooterHeight
	remaining := height - chrome

	// Both body and log are bordered, so each costs its content plus a frame.
	available := remaining - 2*paneFrameHeight
	if available < 2 {
		// No room for two bordered panes. Give everything to the body and drop
		// the log to nothing; the view renders it borderless in this case.
		l.LogHeight = 0
		l.BodyHeight = max(1, remaining-paneFrameHeight)
		l.Degraded = true
		l.ListWidth, l.DetailsWidth = splitWidth(width)
		return l
	}

	// The log never takes more than it wants, nor more than half the space, so
	// the package list always keeps the majority of the screen.
	l.LogHeight = min(wantLog, available/2)
	l.BodyHeight = available - l.LogHeight

	// If the body is too short, take height back from the log before declaring
	// the layout degraded: seeing packages matters more than seeing old output.
	if l.BodyHeight < minBodyHeight {
		deficit := minBodyHeight - l.BodyHeight
		give := min(deficit, max(0, l.LogHeight-minLogHeight))
		l.LogHeight -= give
		l.BodyHeight += give
	}
	if l.BodyHeight < minBodyHeight || l.LogHeight < minLogHeight {
		l.Degraded = true
	}
	l.BodyHeight = max(1, l.BodyHeight)
	l.LogHeight = max(1, l.LogHeight)

	l.ListWidth, l.DetailsWidth = splitWidth(width)
	return l
}

// splitWidth divides the terminal between the list and details panes. Both are
// bordered, so the returned widths are content widths and the two frames are
// already accounted for.
func splitWidth(width int) (listWidth, detailsWidth int) {
	content := width - 2*paneFrameWidth
	if content < 2 {
		return max(1, content), 0
	}

	listWidth = content * listWidthPercent / 100
	listWidth = max(listWidth, min(minListWidth, content-1))
	detailsWidth = content - listWidth

	// A very narrow terminal cannot honour both minimums; the list wins, since
	// without it there is nothing to select and nothing to show details for.
	if detailsWidth < minDetailsWidth {
		detailsWidth = min(minDetailsWidth, content-minListWidth)
		if detailsWidth < 1 {
			detailsWidth = 1
		}
		listWidth = content - detailsWidth
	}
	return max(1, listWidth), max(1, detailsWidth)
}

// TotalHeight is the sum of every pane's outer height. The view asserts this
// equals the terminal height so no pane can push another off-screen.
func (l layout) TotalHeight() int {
	total := l.HeaderHeight + l.SearchHeight + l.StatusHeight + l.FooterHeight
	total += l.BodyHeight + paneFrameHeight
	if l.LogHeight > 0 {
		total += l.LogHeight + paneFrameHeight
	}
	return total
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func clamp(v, lo, hi int) int {
	if lo > hi {
		return lo
	}
	return max(lo, min(v, hi))
}
