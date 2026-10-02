package main

import (
	"strings"
)

// Debian control file parsing.
//
// apt-cache show, apt-cache dumpavail and dpkg-query -s all emit RFC822-like
// "control" records: `Field: value` lines, where a continuation line starts
// with a space or tab and belongs to the preceding field. Records are separated
// by a blank line.
//
// Everything in this file is a pure function over strings so it can be tested
// without apt installed.

// controlRecord is one parsed stanza. Field names are stored canonicalised to
// lower case, since apt is not consistent about capitalisation across sources.
type controlRecord struct {
	fields map[string]string
	order  []string
}

func newControlRecord() *controlRecord {
	return &controlRecord{fields: make(map[string]string)}
}

// Get returns the value of a field, matched case-insensitively.
func (r *controlRecord) Get(field string) string {
	if r == nil || r.fields == nil {
		return ""
	}
	return r.fields[strings.ToLower(field)]
}

// Has reports whether the field is present, even if its value is empty.
func (r *controlRecord) Has(field string) bool {
	if r == nil || r.fields == nil {
		return false
	}
	_, ok := r.fields[strings.ToLower(field)]
	return ok
}

func (r *controlRecord) set(field, value string) {
	key := strings.ToLower(field)
	if _, exists := r.fields[key]; !exists {
		r.order = append(r.order, key)
	}
	r.fields[key] = value
}

// Empty reports whether the record carries no fields at all.
func (r *controlRecord) Empty() bool {
	return r == nil || len(r.fields) == 0
}

// parseControl parses the first control record found in s. Sources such as
// `apt-cache show libssl3` emit one record per available version; callers that
// want a single answer take the first, which apt orders by descending
// preference (the candidate version comes first).
func parseControl(s string) *controlRecord {
	records := parseControlRecords(s, 1)
	if len(records) == 0 {
		return newControlRecord()
	}
	return records[0]
}

// parseControlRecords parses up to limit records from s. A limit <= 0 means
// "all records".
func parseControlRecords(s string, limit int) []*controlRecord {
	var (
		records []*controlRecord
		cur     = newControlRecord()
		field   string
		value   strings.Builder
	)

	// flushField commits the field being accumulated into the current record.
	flushField := func() {
		if field != "" {
			cur.set(field, value.String())
		}
		field = ""
		value.Reset()
	}

	// flushRecord commits the current record and starts a fresh one. It
	// reports whether the caller should stop because limit was reached.
	flushRecord := func() bool {
		flushField()
		if !cur.Empty() {
			records = append(records, cur)
		}
		cur = newControlRecord()
		return limit > 0 && len(records) >= limit
	}

	for _, line := range strings.Split(s, "\n") {
		line = strings.TrimSuffix(line, "\r")

		if strings.TrimSpace(line) == "" {
			// Blank line terminates the record. Leading blank lines and runs
			// of blank lines are harmless because flushRecord drops empties.
			if flushRecord() {
				return records
			}
			continue
		}

		if line[0] == ' ' || line[0] == '\t' {
			// Continuation of the previous field. Keep the newline so that
			// multi-line descriptions and Conffiles survive intact; strip only
			// the single leading space that the format mandates.
			if field != "" {
				value.WriteByte('\n')
				value.WriteString(strings.TrimRight(strings.TrimPrefix(line, " "), " \t"))
			}
			continue
		}

		colon := strings.IndexByte(line, ':')
		if colon < 0 {
			// Not a field and not a continuation: apt error text such as
			// "E: No packages found" reaches us this way. Ignore it.
			continue
		}
		if isAptDiagnostic(line, colon) {
			continue
		}

		flushField()
		field = strings.TrimSpace(line[:colon])
		value.WriteString(strings.TrimSpace(line[colon+1:]))
	}

	flushRecord()
	return records
}

// isAptDiagnostic reports whether a line is one of apt's single-letter
// diagnostics ("E: No packages found", "W: ...", "N: ...") rather than a
// control field. Such lines are syntactically indistinguishable from a field
// named "E", so they are recognised by shape: a one-character name that is an
// upper-case letter. No real control field is named that way.
func isAptDiagnostic(line string, colon int) bool {
	return colon == 1 && line[0] >= 'A' && line[0] <= 'Z'
}

// Description returns the package synopsis (first line) and the long
// description (remaining lines, with the "." blank-line markers converted back
// to real blank lines).
func (r *controlRecord) Description() (synopsis, long string) {
	raw := r.Get("Description")
	if raw == "" {
		return "", ""
	}
	parts := strings.SplitN(raw, "\n", 2)
	synopsis = strings.TrimSpace(parts[0])
	if len(parts) == 1 {
		return synopsis, ""
	}

	var out []string
	for _, line := range strings.Split(parts[1], "\n") {
		// In the control format a lone "." represents an empty line.
		if strings.TrimSpace(line) == "." {
			out = append(out, "")
			continue
		}
		out = append(out, strings.TrimRight(line, " \t"))
	}
	return synopsis, strings.TrimRight(strings.Join(out, "\n"), "\n")
}

// dependency is one entry of a relationship field. Alternatives separated by
// "|" are kept together in Alternatives so the UI can show that a choice
// exists, while Name/Version describe the first alternative.
type dependency struct {
	Name         string
	Arch         string // qualifier from "pkg:any" style names, if present
	Version      string // e.g. ">= 2.36", empty when unconstrained
	Alternatives []dependency
}

// String renders the dependency the way a human reads it in the control file.
func (d dependency) String() string {
	var b strings.Builder
	b.WriteString(d.Display())
	for _, alt := range d.Alternatives {
		b.WriteString(" | ")
		b.WriteString(alt.Display())
	}
	return b.String()
}

// Display renders just this atom, without alternatives.
func (d dependency) Display() string {
	var b strings.Builder
	b.WriteString(d.Name)
	if d.Arch != "" {
		b.WriteByte(':')
		b.WriteString(d.Arch)
	}
	if d.Version != "" {
		b.WriteString(" (")
		b.WriteString(d.Version)
		b.WriteByte(')')
	}
	return b.String()
}

// parseDepends parses a relationship field such as
//
//	libc6 (>= 2.36), libssl3 (>= 3.0.0) | libssl1.1, debconf | debconf-2.0
//
// into one dependency per comma-separated group. Alternatives inside a group
// are attached to the group's first entry. Whitespace and line folding
// introduced by the control format are tolerated.
func parseDepends(field string) []dependency {
	field = strings.TrimSpace(field)
	if field == "" {
		return nil
	}

	var deps []dependency
	for _, group := range strings.Split(field, ",") {
		alternatives := strings.Split(group, "|")

		var parsed []dependency
		for _, atom := range alternatives {
			if d, ok := parseDependencyAtom(atom); ok {
				parsed = append(parsed, d)
			}
		}
		if len(parsed) == 0 {
			continue
		}

		head := parsed[0]
		head.Alternatives = parsed[1:]
		deps = append(deps, head)
	}
	return deps
}

// parseDependencyAtom parses a single "name:arch (>= version)" term.
func parseDependencyAtom(atom string) (dependency, bool) {
	// Collapse the newlines that folded control fields leave behind.
	atom = strings.TrimSpace(strings.ReplaceAll(atom, "\n", " "))
	if atom == "" {
		return dependency{}, false
	}

	var d dependency

	if open := strings.IndexByte(atom, '('); open >= 0 {
		constraint := atom[open+1:]
		if end := strings.IndexByte(constraint, ')'); end >= 0 {
			constraint = constraint[:end]
		}
		d.Version = strings.Join(strings.Fields(constraint), " ")
		atom = strings.TrimSpace(atom[:open])
	}

	// Architecture qualifier lists such as "[amd64 arm64]" and build-profile
	// restrictions such as "<!nocheck>" are not part of the package name.
	if bracket := strings.IndexByte(atom, '['); bracket >= 0 {
		atom = strings.TrimSpace(atom[:bracket])
	}
	if angle := strings.IndexByte(atom, '<'); angle >= 0 {
		atom = strings.TrimSpace(atom[:angle])
	}

	name := strings.Fields(atom)
	if len(name) == 0 {
		return dependency{}, false
	}
	d.Name = name[0]

	if colon := strings.IndexByte(d.Name, ':'); colon >= 0 {
		d.Arch = d.Name[colon+1:]
		d.Name = d.Name[:colon]
	}
	if d.Name == "" {
		return dependency{}, false
	}
	return d, true
}

// relationFields are the control fields toola surfaces in the details pane, in
// display order. Depends and Pre-Depends come first because they are what an
// administrator checks before removing something.
var relationFields = []string{
	"Pre-Depends",
	"Depends",
	"Recommends",
	"Suggests",
	"Enhances",
	"Provides",
	"Conflicts",
	"Breaks",
	"Replaces",
}

// relationGroup is a relationship field together with its parsed entries.
type relationGroup struct {
	Field string
	Deps  []dependency
}

// relations extracts every non-empty relationship group from the record, in
// relationFields order.
func (r *controlRecord) relations() []relationGroup {
	var groups []relationGroup
	for _, field := range relationFields {
		deps := parseDepends(r.Get(field))
		if len(deps) == 0 {
			continue
		}
		groups = append(groups, relationGroup{Field: field, Deps: deps})
	}
	return groups
}
