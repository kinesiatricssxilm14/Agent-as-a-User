package ui

import "github.com/toolg/toolg/internal/conflict"

// side identifies one of the three-way merge panels.
type side int

const (
	sideOurs side = iota
	sideBase
	sideResult
	sideTheirs
)

func (s side) String() string {
	switch s {
	case sideOurs:
		return "OURS"
	case sideBase:
		return "BASE"
	case sideResult:
		return "RESULT"
	case sideTheirs:
		return "THEIRS"
	}
	return "?"
}

// cell is one panel's content for one screen row.
type cell struct {
	// text is the line content. Empty text with filler set draws a padding row.
	text string
	// lineNo is the 1-based line number to show in the gutter, or 0 for none.
	lineNo int
	// filler marks a row that exists only to keep the three panels aligned,
	// because one side of a conflict has fewer lines than another.
	filler bool
}

// row is a single horizontal slice across all panels. Rendering row-by-row is
// what guarantees that a conflict's ours and theirs text sit next to each other
// on the same screen line, which is what makes the comparison readable.
type row struct {
	ours   cell
	base   cell
	result cell
	theirs cell

	// blockIndex is the conflict this row belongs to, or -1 for context lines
	// outside any conflict.
	blockIndex int
	// conflict is true when the row is inside a conflict block.
	conflict bool
	// resolved mirrors the block's resolution state, used to colour the result.
	resolved bool
	// header marks the first row of a conflict block, which carries the block
	// label instead of file content.
	header bool
	// choice is the block's current strategy, for the header label.
	choice conflict.Choice
}

// layout is the full set of rows for a parsed file, plus an index from block
// number to the row where that block starts.
type layout struct {
	rows []row
	// blockRow maps a block index to its first row, so jumping between
	// conflicts can scroll straight to it.
	blockRow []int
}

// buildLayout flattens a conflict file into aligned rows.
//
// Context lines appear in every panel with their real line numbers, which keeps
// the panels visually locked together while scrolling. Conflict blocks expand
// to the height of their tallest side; shorter sides get filler rows so nothing
// drifts out of alignment.
func buildLayout(f *conflict.File, showBase bool) *layout {
	l := &layout{blockRow: make([]int, len(f.Blocks))}

	// Line numbers are tracked per side, since each side numbers its own
	// version of the file independently.
	oursNo, theirsNo, baseNo, resultNo := 0, 0, 0, 0

	// The result panel's line numbers must match what will land on disk, so
	// only resolved content advances the counter.
	appendRow := func(r row) { l.rows = append(l.rows, r) }

	for _, seg := range f.Segments() {
		if seg.Block == nil {
			for _, line := range seg.Lines {
				oursNo++
				theirsNo++
				baseNo++
				resultNo++
				appendRow(row{
					ours:       cell{text: line, lineNo: oursNo},
					base:       cell{text: line, lineNo: baseNo},
					result:     cell{text: line, lineNo: resultNo},
					theirs:     cell{text: line, lineNo: theirsNo},
					blockIndex: -1,
				})
			}
			continue
		}

		b := seg.Block
		l.blockRow[b.Index] = len(l.rows)

		// A header row names the block and shows its current choice, giving
		// the user an anchor even when a side is empty.
		appendRow(row{
			blockIndex: b.Index,
			conflict:   true,
			resolved:   b.Choice.Resolved(),
			header:     true,
			choice:     b.Choice,
		})

		res := b.Result()
		height := len(b.Ours)
		if len(b.Theirs) > height {
			height = len(b.Theirs)
		}
		if len(res) > height {
			height = len(res)
		}
		if showBase && b.HasBase && len(b.Base) > height {
			height = len(b.Base)
		}
		// A block where every side is empty still needs one row so it can be
		// selected and shown.
		if height == 0 {
			height = 1
		}

		for i := 0; i < height; i++ {
			r := row{
				blockIndex: b.Index,
				conflict:   true,
				resolved:   b.Choice.Resolved(),
				choice:     b.Choice,
			}
			if i < len(b.Ours) {
				oursNo++
				r.ours = cell{text: b.Ours[i], lineNo: oursNo}
			} else {
				r.ours = cell{filler: true}
			}
			if i < len(b.Theirs) {
				theirsNo++
				r.theirs = cell{text: b.Theirs[i], lineNo: theirsNo}
			} else {
				r.theirs = cell{filler: true}
			}
			if showBase && b.HasBase {
				if i < len(b.Base) {
					baseNo++
					r.base = cell{text: b.Base[i], lineNo: baseNo}
				} else {
					r.base = cell{filler: true}
				}
			}
			if i < len(res) {
				resultNo++
				r.result = cell{text: res[i], lineNo: resultNo}
			} else {
				r.result = cell{filler: true}
			}
			appendRow(r)
		}
	}
	return l
}

// cellFor returns the cell a given panel contributes to a row.
func (r row) cellFor(s side) cell {
	switch s {
	case sideOurs:
		return r.ours
	case sideBase:
		return r.base
	case sideResult:
		return r.result
	case sideTheirs:
		return r.theirs
	}
	return cell{}
}
