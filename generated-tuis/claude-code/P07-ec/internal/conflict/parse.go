// Package conflict parses and rewrites files containing Git conflict markers.
//
// A file is modelled as an ordered sequence of segments. Plain segments hold
// lines that live outside any conflict and are never modified. Conflict
// segments hold the ours/base/theirs sides of one conflict block plus the
// resolution the user has chosen for it. Rendering the segments back out
// reproduces the file byte-for-byte outside the conflict blocks, which is what
// "preserve non-conflict lines" requires.
package conflict

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"strings"
)

// Marker prefixes as produced by git's merge drivers. Git pads the marker to
// exactly seven characters, so a prefix test is enough to recognise them while
// still tolerating the label that follows.
const (
	markerOurs  = "<<<<<<<"
	markerBase  = "|||||||"
	markerSep   = "======="
	markerTheir = ">>>>>>>"
)

// Choice is the resolution strategy selected for a single conflict block.
type Choice int

const (
	// ChoiceUnresolved means the user has not decided yet; the block still
	// renders with its markers when written back.
	ChoiceUnresolved Choice = iota
	// ChoiceOurs keeps only the ours side (HEAD / the branch being merged into).
	ChoiceOurs
	// ChoiceTheirs keeps only the theirs side (the incoming branch).
	ChoiceTheirs
	// ChoiceBoth keeps ours followed by theirs.
	ChoiceBoth
	// ChoiceNone drops the whole conflict block.
	ChoiceNone
	// ChoiceCustom uses Block.Custom verbatim, allowing hand editing.
	ChoiceCustom
)

// String returns a short label suitable for status bars and panel headers.
func (c Choice) String() string {
	switch c {
	case ChoiceOurs:
		return "ours"
	case ChoiceTheirs:
		return "theirs"
	case ChoiceBoth:
		return "both"
	case ChoiceNone:
		return "none"
	case ChoiceCustom:
		return "edited"
	default:
		return "unresolved"
	}
}

// Resolved reports whether a choice contributes a final result.
func (c Choice) Resolved() bool { return c != ChoiceUnresolved }

// Block is one conflict region.
type Block struct {
	// Index is the zero-based position of this block among the conflicts in
	// the file, in file order.
	Index int

	// OursLabel and TheirsLabel are the annotations git wrote next to the
	// opening and closing markers, e.g. "HEAD" and "feature". They may be
	// empty when a marker carried no label.
	OursLabel   string
	BaseLabel   string
	TheirsLabel string

	// HasBase records whether the block used diff3 style and therefore
	// carried a common-ancestor section.
	HasBase bool

	// Ours, Base and Theirs hold the lines of each side without markers.
	Ours   []string
	Base   []string
	Theirs []string

	// Choice is the selected strategy, and Custom the lines used when Choice
	// is ChoiceCustom.
	Choice Choice
	Custom []string

	// StartLine is the 1-based line number of the opening marker in the file
	// as it was parsed. It is only meaningful for the original file and is
	// used for user-facing messages.
	StartLine int
}

// Result returns the lines this block contributes to the resolved file.
func (b *Block) Result() []string {
	switch b.Choice {
	case ChoiceOurs:
		return b.Ours
	case ChoiceTheirs:
		return b.Theirs
	case ChoiceBoth:
		out := make([]string, 0, len(b.Ours)+len(b.Theirs))
		out = append(out, b.Ours...)
		out = append(out, b.Theirs...)
		return out
	case ChoiceNone:
		return nil
	case ChoiceCustom:
		return b.Custom
	default:
		// Unresolved blocks keep their markers so that writing back a
		// partially resolved file leaves a still-valid conflict file.
		return b.markerLines()
	}
}

// markerLines reconstructs the block exactly as it appeared in the file.
func (b *Block) markerLines() []string {
	out := make([]string, 0, len(b.Ours)+len(b.Base)+len(b.Theirs)+4)
	out = append(out, joinMarker(markerOurs, b.OursLabel))
	out = append(out, b.Ours...)
	if b.HasBase {
		out = append(out, joinMarker(markerBase, b.BaseLabel))
		out = append(out, b.Base...)
	}
	out = append(out, markerSep)
	out = append(out, b.Theirs...)
	out = append(out, joinMarker(markerTheir, b.TheirsLabel))
	return out
}

func joinMarker(marker, label string) string {
	if label == "" {
		return marker
	}
	return marker + " " + label
}

// segment is either a run of untouched lines or a pointer to a conflict block.
type segment struct {
	lines []string // set when block == nil
	block *Block   // set for conflict segments
}

// File is a parsed conflict file.
type File struct {
	// Path is the path the file was read from, as given by the caller.
	Path string

	segments []segment

	// Blocks indexes the conflict blocks in file order. The pointers alias
	// the ones held by segments, so mutating a block through this slice
	// affects rendering.
	Blocks []*Block

	// trailingNewline records whether the source ended with a line
	// terminator, so that Render can reproduce it.
	trailingNewline bool

	// crlf records that the file used \r\n endings, so Render restores them.
	crlf bool
}

// ParseError describes a malformed conflict file.
type ParseError struct {
	Line int
	Msg  string
}

func (e *ParseError) Error() string {
	return fmt.Sprintf("line %d: %s", e.Line, e.Msg)
}

// Parse reads a conflict file from r.
//
// Lines outside conflict markers are captured verbatim. A marker sequence that
// does not follow the ours -> [base] -> separator -> theirs order is reported
// as a ParseError rather than being silently reinterpreted, because guessing
// at a damaged file risks destroying the user's work on write-back.
func Parse(path string, r io.Reader) (*File, error) {
	data, err := io.ReadAll(r)
	if err != nil {
		return nil, err
	}
	return ParseBytes(path, data)
}

// ParseBytes parses an in-memory conflict file.
func ParseBytes(path string, data []byte) (*File, error) {
	f := &File{Path: path}

	// Detect and normalise line endings so that parsing only deals with \n.
	// The dominant style is restored on render.
	if bytes.Contains(data, []byte("\r\n")) {
		f.crlf = true
		data = bytes.ReplaceAll(data, []byte("\r\n"), []byte("\n"))
	}
	f.trailingNewline = len(data) > 0 && data[len(data)-1] == '\n'

	var lines []string
	sc := bufio.NewScanner(bytes.NewReader(data))
	sc.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	for sc.Scan() {
		lines = append(lines, sc.Text())
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}

	// state machine over the marker grammar
	const (
		stOutside = iota
		stOurs
		stBase
		stTheirs
	)
	state := stOutside
	var plain []string
	var cur *Block

	flushPlain := func() {
		if len(plain) > 0 {
			f.segments = append(f.segments, segment{lines: plain})
			plain = nil
		}
	}

	for i, line := range lines {
		lineNo := i + 1
		switch {
		case strings.HasPrefix(line, markerOurs):
			if state != stOutside {
				return nil, &ParseError{lineNo, "unexpected '<<<<<<<' inside a conflict block"}
			}
			flushPlain()
			cur = &Block{
				Index:     len(f.Blocks),
				OursLabel: label(line, markerOurs),
				StartLine: lineNo,
			}
			state = stOurs

		case strings.HasPrefix(line, markerBase) && state == stOurs:
			cur.HasBase = true
			cur.BaseLabel = label(line, markerBase)
			state = stBase

		case strings.HasPrefix(line, markerSep) && (state == stOurs || state == stBase):
			state = stTheirs

		case strings.HasPrefix(line, markerTheir) && state == stTheirs:
			cur.TheirsLabel = label(line, markerTheir)
			f.Blocks = append(f.Blocks, cur)
			f.segments = append(f.segments, segment{block: cur})
			cur = nil
			state = stOutside

		default:
			switch state {
			case stOutside:
				plain = append(plain, line)
			case stOurs:
				cur.Ours = append(cur.Ours, line)
			case stBase:
				cur.Base = append(cur.Base, line)
			case stTheirs:
				cur.Theirs = append(cur.Theirs, line)
			}
		}
	}

	if state != stOutside {
		return nil, &ParseError{cur.StartLine, "unterminated conflict block"}
	}
	flushPlain()
	return f, nil
}

// label extracts the annotation following a marker, e.g. "HEAD" from
// "<<<<<<< HEAD".
func label(line, marker string) string {
	return strings.TrimSpace(strings.TrimPrefix(line, marker))
}

// HasConflicts reports whether any conflict markers were found.
func (f *File) HasConflicts() bool { return len(f.Blocks) > 0 }

// UnresolvedCount returns how many blocks still lack a resolution.
func (f *File) UnresolvedCount() int {
	n := 0
	for _, b := range f.Blocks {
		if !b.Choice.Resolved() {
			n++
		}
	}
	return n
}

// ResolveAll applies one choice to every block, which backs the "apply to all"
// shortcuts.
func (f *File) ResolveAll(c Choice) {
	for _, b := range f.Blocks {
		b.Choice = c
	}
}

// ResultLines renders the resolved file as a slice of lines.
func (f *File) ResultLines() []string {
	var out []string
	for _, seg := range f.segments {
		if seg.block == nil {
			out = append(out, seg.lines...)
			continue
		}
		out = append(out, seg.block.Result()...)
	}
	return out
}

// Render serialises the resolved file, restoring the original line-ending
// style and trailing-newline behaviour.
func (f *File) Render() []byte {
	lines := f.ResultLines()
	nl := "\n"
	if f.crlf {
		nl = "\r\n"
	}
	var buf bytes.Buffer
	for i, l := range lines {
		buf.WriteString(l)
		if i < len(lines)-1 || f.trailingNewline {
			buf.WriteString(nl)
		}
	}
	return buf.Bytes()
}

// ResultBlockRange reports the line span, within ResultLines, that the given
// block occupies. The returned start is a 0-based index and count may be zero
// for a block resolved to nothing. ok is false if the index is out of range.
//
// The UI uses this to scroll the result panel to the block under the cursor
// and to highlight the lines that block contributed.
func (f *File) ResultBlockRange(blockIndex int) (start, count int, ok bool) {
	pos := 0
	for _, seg := range f.segments {
		if seg.block == nil {
			pos += len(seg.lines)
			continue
		}
		n := len(seg.block.Result())
		if seg.block.Index == blockIndex {
			return pos, n, true
		}
		pos += n
	}
	return 0, 0, false
}

// Segment is one piece of the file: either a run of untouched lines or a
// conflict block, never both.
type Segment struct {
	// Lines holds the verbatim lines of a non-conflict run. It is nil when
	// Block is set.
	Lines []string
	// Block points at a conflict block, or is nil for a plain run.
	Block *Block
}

// Segments returns the file's structure in order, so callers can render the
// original interleaving of plain text and conflicts without re-parsing.
//
// The returned slice is a fresh copy, but Block pointers alias the file's own
// blocks, so resolving through them affects Render.
func (f *File) Segments() []Segment {
	out := make([]Segment, 0, len(f.segments))
	for _, seg := range f.segments {
		out = append(out, Segment{Lines: seg.lines, Block: seg.block})
	}
	return out
}
