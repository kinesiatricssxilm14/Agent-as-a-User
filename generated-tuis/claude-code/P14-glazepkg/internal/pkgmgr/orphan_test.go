package pkgmgr

import (
	"reflect"
	"testing"
)

// snap builds a dependency graph from a name -> requirements map.
func snap(g map[string][]string) Snapshot {
	s := Snapshot{}
	for name, reqs := range g {
		s[normalizeName(name)] = node{Version: "1.0", Requires: normalizeAll(reqs)}
	}
	return s
}

func liveOf(s Snapshot, removed ...string) map[string]bool {
	live := map[string]bool{}
	for n := range s {
		live[n] = true
	}
	for _, r := range normalizeAll(removed) {
		delete(live, r)
	}
	return live
}

func TestOrphansFrom(t *testing.T) {
	tests := []struct {
		name   string
		graph  map[string][]string
		remove []string
		want   []string
	}{
		{
			name: "chain collapses fully",
			graph: map[string][]string{
				"flask":      {"jinja2", "click", "werkzeug"},
				"jinja2":     {"markupsafe"},
				"markupsafe": nil,
				"click":      nil,
				"werkzeug":   {"markupsafe"},
			},
			remove: []string{"flask"},
			want:   []string{"click", "jinja2", "markupsafe", "werkzeug"},
		},
		{
			name: "shared dependency survives",
			graph: map[string][]string{
				"flask":      {"jinja2"},
				"other":      {"jinja2"},
				"jinja2":     {"markupsafe"},
				"markupsafe": nil,
			},
			remove: []string{"flask"},
			want:   nil, // jinja2 still needed by other, so markupsafe stays too
		},
		{
			name: "core packages are never removed",
			graph: map[string][]string{
				"something":  {"pip", "setuptools", "wheel"},
				"pip":        nil,
				"setuptools": nil,
				"wheel":      nil,
			},
			remove: []string{"something"},
			want:   nil,
		},
		{
			name: "diamond collapses when both parents go",
			graph: map[string][]string{
				"a":      {"shared"},
				"b":      {"shared"},
				"shared": {"leaf"},
				"leaf":   nil,
			},
			remove: []string{"a", "b"},
			want:   []string{"leaf", "shared"},
		},
		{
			name: "dependency that is also explicitly removed is not double-listed",
			graph: map[string][]string{
				"a":   {"dep"},
				"dep": nil,
			},
			remove: []string{"a", "dep"},
			want:   nil,
		},
		{
			name: "cycle does not hang",
			graph: map[string][]string{
				"root": {"x"},
				"x":    {"y"},
				"y":    {"x"},
			},
			remove: []string{"root"},
			want:   []string{"x", "y"},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			before := snap(tc.graph)
			live := liveOf(before, tc.remove...)
			got := orphansFrom(before, before, live, tc.remove)
			if len(got) == 0 && len(tc.want) == 0 {
				return
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Errorf("orphansFrom() = %v, want %v", got, tc.want)
			}
		})
	}
}
