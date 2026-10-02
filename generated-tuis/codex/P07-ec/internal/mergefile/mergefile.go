// Package mergefile parses and resolves files containing Git conflict markers.
package mergefile

import (
	"fmt"
	"os"
	"strings"
)

type Choice int

const (
	Unresolved Choice = iota
	Ours
	Theirs
	Both
	None
)

func (c Choice) String() string {
	switch c {
	case Ours:
		return "ours"
	case Theirs:
		return "theirs"
	case Both:
		return "both"
	case None:
		return "none"
	default:
		return "unresolved"
	}
}

type Conflict struct {
	Ours       []string
	Base       []string
	Theirs     []string
	OursLabel  string
	TheirLabel string
	StartLine  int
	Choice     Choice
}

type Part struct {
	Text     []string
	Conflict *Conflict
}

type File struct {
	Parts       []Part
	Conflicts   []*Conflict
	HadFinalEOL bool
}

func Parse(data []byte) (*File, error) {
	text := string(data)
	lines := splitLines(text)
	f := &File{HadFinalEOL: strings.HasSuffix(text, "\n")}
	var plain []string
	flush := func() {
		if len(plain) > 0 {
			f.Parts = append(f.Parts, Part{Text: append([]string(nil), plain...)})
			plain = nil
		}
	}

	for i := 0; i < len(lines); {
		if !isMarker(lines[i], "<<<<<<<") {
			plain = append(plain, lines[i])
			i++
			continue
		}
		flush()
		start := i + 1
		c := &Conflict{OursLabel: markerLabel(lines[i], "<<<<<<<"), StartLine: start}
		i++
		phase := 0 // ours, base, theirs
		foundSep, foundEnd := false, false
		for i < len(lines) {
			line := lines[i]
			switch {
			case phase == 0 && isMarker(line, "|||||||"):
				phase = 1
				i++
				continue
			case (phase == 0 || phase == 1) && isMarker(line, "======="):
				phase = 2
				foundSep = true
				i++
				continue
			case phase == 2 && isMarker(line, ">>>>>>>"):
				c.TheirLabel = markerLabel(line, ">>>>>>>")
				foundEnd = true
				i++
			}
			if foundEnd {
				break
			}
			switch phase {
			case 0:
				c.Ours = append(c.Ours, line)
			case 1:
				c.Base = append(c.Base, line)
			case 2:
				c.Theirs = append(c.Theirs, line)
			}
			i++
		}
		if !foundSep || !foundEnd {
			return nil, fmt.Errorf("malformed conflict beginning at line %d", start)
		}
		f.Conflicts = append(f.Conflicts, c)
		f.Parts = append(f.Parts, Part{Conflict: c})
	}
	flush()
	return f, nil
}

func Read(path string) (*File, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return Parse(data)
}

func (f *File) AllResolved() bool {
	for _, c := range f.Conflicts {
		if c.Choice == Unresolved {
			return false
		}
	}
	return true
}

func (f *File) ResolveAll(choice Choice) {
	for _, c := range f.Conflicts {
		c.Choice = choice
	}
}

func resolution(c *Conflict, unresolvedMarkers bool) []string {
	switch c.Choice {
	case Ours:
		return c.Ours
	case Theirs:
		return c.Theirs
	case Both:
		return append(append([]string(nil), c.Ours...), c.Theirs...)
	case None:
		return nil
	default:
		if !unresolvedMarkers {
			return nil
		}
		out := []string{"<<<<<<< " + defaultLabel(c.OursLabel, "OURS") + "\n"}
		out = append(out, c.Ours...)
		out = append(out, "=======\n")
		out = append(out, c.Theirs...)
		out = append(out, ">>>>>>> "+defaultLabel(c.TheirLabel, "THEIRS")+"\n")
		return out
	}
}

func (f *File) Render(unresolvedMarkers bool) string {
	var b strings.Builder
	for _, p := range f.Parts {
		if p.Conflict == nil {
			for _, line := range p.Text {
				b.WriteString(line)
			}
			continue
		}
		for _, line := range resolution(p.Conflict, unresolvedMarkers) {
			b.WriteString(line)
		}
	}
	return b.String()
}

func (f *File) Write(path string, mode os.FileMode) error {
	if !f.AllResolved() {
		return fmt.Errorf("resolve all conflicts before writing")
	}
	return os.WriteFile(path, []byte(f.Render(false)), mode)
}

func splitLines(s string) []string {
	if s == "" {
		return nil
	}
	lines := strings.SplitAfter(s, "\n")
	if lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

func markerText(line string) string     { return strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r") }
func isMarker(line, marker string) bool { return strings.HasPrefix(markerText(line), marker) }
func markerLabel(line, marker string) string {
	return strings.TrimSpace(strings.TrimPrefix(markerText(line), marker))
}
func defaultLabel(v, fallback string) string {
	if v == "" {
		return fallback
	}
	return v
}
