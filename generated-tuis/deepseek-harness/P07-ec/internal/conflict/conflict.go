// Package conflict parses and manipulates files that are in a Git merge
// conflict state. It understands the standard Git conflict markers:
//
//	<<<<<<< <ours-label>
//	<ours content>
//	=======
//	<theirs content>
//	>>>>>>> <theirs-label>
//
// and is able to replace only the conflict blocks while preserving every
// non-conflict line byte-for-byte.
package conflict

import (
	"fmt"
	"strings"
)

// Resolution describes how a single conflict block was (or will be) resolved.
type Resolution int

const (
	// ResolveUnresolved means the conflict block has not been resolved yet.
	ResolveUnresolved Resolution = iota
	// ResolveOurs keeps only the "ours" (HEAD / current branch) side.
	ResolveOurs
	// ResolveTheirs keeps only the "theirs" (incoming branch) side.
	ResolveTheirs
	// ResolveBoth keeps the ours side followed by the theirs side.
	ResolveBoth
	// ResolveNone discards the conflict block entirely (empty result).
	ResolveNone
)

// String returns a human readable name for the resolution.
func (r Resolution) String() string {
	switch r {
	case ResolveOurs:
		return "ours"
	case ResolveTheirs:
		return "theirs"
	case ResolveBoth:
		return "both"
	case ResolveNone:
		return "none"
	default:
		return "unresolved"
	}
}

// Kind identifies what a chunk of a file represents.
type Kind int

const (
	// KindPlain is a run of ordinary (non-conflict) lines.
	KindPlain Kind = iota
	// KindConflict is a single <<<<<<< / ======= / >>>>>>> block.
	KindConflict
)

// Chunk is either a run of plain lines or a single conflict block.
type Chunk struct {
	Kind Kind

	// Lines holds the content of a plain chunk.
	Lines []string

	// Conflict content.
	Ours        []string
	Theirs      []string
	OursLabel   string
	TheirsLabel string

	// Resolution state (meaningful only for KindConflict chunks).
	Resolution Resolution
	Result     []string
}

// Resolved reports whether a conflict chunk has been resolved.
func (c Chunk) Resolved() bool {
	return c.Kind == KindConflict && c.Resolution != ResolveUnresolved
}

// RawBlock returns the original conflict-marker lines for a conflict chunk,
// exactly as they would appear if the block were left unresolved.
func (c Chunk) RawBlock() []string {
	return rawBlock(c)
}

// File is a parsed file together with its resolution state.
type File struct {
	Path   string
	Lines  []string // raw lines, in order, exactly as read
	Chunks []Chunk
}

// Parse parses raw file content and returns a File. Line terminators are
// preserved so that non-conflict content can be written back unchanged.
func Parse(path, content string) (*File, error) {
	lines := splitLines(content)
	f := &File{Path: path, Lines: lines}

	var plain []string
	i := 0
	for i < len(lines) {
		line := lines[i]
		if isMarkerStart(line) {
			if len(plain) > 0 {
				f.Chunks = append(f.Chunks, Chunk{Kind: KindPlain, Lines: plain})
				plain = nil
			}
			chunk, next, err := parseConflict(lines, i)
			if err != nil {
				// A malformed block should not abort the whole parse; treat
				// what we could not parse as plain text so nothing is lost.
				if len(chunk.Ours) == 0 && len(chunk.Theirs) == 0 {
					plain = append(plain, line)
					i++
					continue
				}
			}
			f.Chunks = append(f.Chunks, chunk)
			i = next
			continue
		}
		plain = append(plain, line)
		i++
	}
	if len(plain) > 0 {
		f.Chunks = append(f.Chunks, Chunk{Kind: KindPlain, Lines: plain})
	}
	return f, nil
}

// parseConflict parses a single conflict block starting at lines[start], which
// must be a "<<" marker. It returns the chunk and the index just past the
// ">>" marker.
func parseConflict(lines []string, start int) (Chunk, int, error) {
	c := Chunk{Kind: KindConflict}
	c.OursLabel = markerLabel(lines[start], "<<<<<<<")
	i := start + 1

	for i < len(lines) && !isMarkerSep(lines[i]) {
		c.Ours = append(c.Ours, lines[i])
		i++
	}
	if i >= len(lines) {
		return c, i, fmt.Errorf("missing ======= marker")
	}
	i++ // skip "======="

	for i < len(lines) && !isMarkerEnd(lines[i]) {
		c.Theirs = append(c.Theirs, lines[i])
		i++
	}
	if i >= len(lines) {
		return c, i, fmt.Errorf("missing >>>>>>> marker")
	}
	c.TheirsLabel = markerLabel(lines[i], ">>>>>>>")
	i++ // skip ">>>>>>>"
	return c, i, nil
}

// ConflictChunkIndices returns the indices (into Chunks) of every conflict
// chunk, in file order.
func (f *File) ConflictChunkIndices() []int {
	var out []int
	for i, c := range f.Chunks {
		if c.Kind == KindConflict {
			out = append(out, i)
		}
	}
	return out
}

// ConflictCount returns the number of conflict blocks in the file.
func (f *File) ConflictCount() int {
	return len(f.ConflictChunkIndices())
}

// HasConflicts reports whether the file contains any conflict markers.
func (f *File) HasConflicts() bool {
	return f.ConflictCount() > 0
}

// AllResolved reports whether every conflict block has been resolved.
func (f *File) AllResolved() bool {
	for _, c := range f.Chunks {
		if c.Kind == KindConflict && !c.Resolved() {
			return false
		}
	}
	return true
}

// UnresolvedCount returns the number of unresolved conflict blocks.
func (f *File) UnresolvedCount() int {
	n := 0
	for _, c := range f.Chunks {
		if c.Kind == KindConflict && !c.Resolved() {
			n++
		}
	}
	return n
}

// Resolve applies a resolution strategy to the conflict block at the given
// chunk index. Plain chunks are ignored.
func (f *File) Resolve(chunkIndex int, r Resolution) {
	if chunkIndex < 0 || chunkIndex >= len(f.Chunks) {
		return
	}
	c := &f.Chunks[chunkIndex]
	if c.Kind != KindConflict {
		return
	}
	c.Resolution = r
	switch r {
	case ResolveOurs:
		c.Result = append([]string(nil), c.Ours...)
	case ResolveTheirs:
		c.Result = append([]string(nil), c.Theirs...)
	case ResolveBoth:
		c.Result = append(append([]string(nil), c.Ours...), c.Theirs...)
	case ResolveNone:
		c.Result = []string{}
	case ResolveUnresolved:
		c.Result = nil
	}
}

// ResolveAll applies a strategy to every conflict block.
func (f *File) ResolveAll(r Resolution) {
	for i := range f.Chunks {
		f.Resolve(i, r)
	}
}

// Reset resets a conflict block back to its unresolved state.
func (f *File) Reset(chunkIndex int) {
	f.Resolve(chunkIndex, ResolveUnresolved)
}

// Render reconstructs the file content. Resolved conflict blocks are replaced
// with their result; unresolved conflict blocks are written back using the
// original markers so the file remains in a valid conflict state. Non-conflict
// lines are always preserved exactly.
func (f *File) Render() string {
	var out []string
	for _, c := range f.Chunks {
		switch c.Kind {
		case KindPlain:
			out = append(out, c.Lines...)
		case KindConflict:
			if c.Resolved() {
				out = append(out, c.Result...)
			} else {
				out = append(out, rawBlock(c)...)
			}
		}
	}
	return strings.Join(out, "\n")
}

// rawBlock reconstructs the original conflict markers for an unresolved chunk.
func rawBlock(c Chunk) []string {
	var b []string
	ours := c.OursLabel
	if ours == "" {
		ours = "HEAD"
	}
	theirs := c.TheirsLabel
	if theirs == "" {
		theirs = "incoming"
	}
	b = append(b, "<<<<<<< "+ours)
	b = append(b, c.Ours...)
	b = append(b, "=======")
	b = append(b, c.Theirs...)
	b = append(b, ">>>>>>> "+theirs)
	return b
}

// splitLines splits content on '\n', preserving a trailing empty element so
// that a file ending in a newline is reconstructed identically.
func splitLines(s string) []string {
	return strings.Split(s, "\n")
}

func isMarkerStart(s string) bool { return isMarker(s, "<<<<<<<") }
func isMarkerSep(s string) bool   { return isMarker(s, "=======") }
func isMarkerEnd(s string) bool   { return isMarker(s, ">>>>>>>") }

// isMarker reports whether line begins with the exact Git marker followed by
// end-of-line or a space/tab (the label). This avoids matching content lines
// such as "<<<<<<<<" (eight '<') which are not valid Git markers.
func isMarker(line, marker string) bool {
	if !strings.HasPrefix(line, marker) {
		return false
	}
	rest := line[len(marker):]
	if rest == "" {
		return true
	}
	return rest[0] == ' ' || rest[0] == '\t'
}

func markerLabel(line, marker string) string {
	return strings.TrimSpace(line[len(marker):])
}
