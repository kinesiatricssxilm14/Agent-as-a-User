package pkgmgr

import (
	"strconv"
	"strings"
)

// block is one RFC 822 style metadata record, as printed by `pip show` and
// `apt-cache show`. Keys are matched case-insensitively; continuation lines are
// folded into the preceding value.
type block struct {
	keys   []string
	values map[string]string
}

func (b block) get(key string) string {
	if b.values == nil {
		return ""
	}
	return strings.TrimSpace(b.values[strings.ToLower(key)])
}

// parseRFC822 splits text into records separated by blank lines or by the
// "---" separator pip prints between `pip show` results.
func parseRFC822(text string) []block {
	var (
		out     []block
		cur     = newBlock()
		lastKey string
	)
	flush := func() {
		if len(cur.keys) > 0 {
			out = append(out, cur)
		}
		cur = newBlock()
		lastKey = ""
	}

	for _, raw := range strings.Split(text, "\n") {
		line := strings.TrimRight(raw, "\r")
		trimmed := strings.TrimSpace(line)

		if trimmed == "" || trimmed == "---" {
			flush()
			continue
		}
		// A continuation line is *indented*. Test the leading whitespace only:
		// `pip show` emits empty fields as "Home-page: " with a trailing space,
		// and comparing against the fully trimmed line would misread those as
		// continuations of the field before them.
		if isIndented(line) && lastKey != "" {
			cur.values[lastKey] += "\n" + trimmed
			continue
		}
		idx := strings.IndexByte(line, ':')
		if idx <= 0 {
			if lastKey != "" {
				cur.values[lastKey] += "\n" + trimmed
			}
			continue
		}
		key := strings.ToLower(strings.TrimSpace(line[:idx]))
		value := strings.TrimSpace(line[idx+1:])
		// A repeated Name starts a new record (successive `pip show` outputs are
		// separated by "---", but be forgiving).
		if _, exists := cur.values[key]; exists && key == "name" {
			flush()
		}
		if _, exists := cur.values[key]; !exists {
			cur.keys = append(cur.keys, key)
			cur.values[key] = value
		} else {
			cur.values[key] += "\n" + value
		}
		lastKey = key
	}
	flush()
	return out
}

// isIndented reports whether the line begins with whitespace.
func isIndented(line string) bool {
	return line != "" && (line[0] == ' ' || line[0] == '\t')
}

func newBlock() block { return block{values: map[string]string{}} }

// splitList parses a comma or whitespace separated metadata list. pip prints
// "Requires:" with no value when there are none.
func splitList(s string) []string {
	s = strings.TrimSpace(s)
	if s == "" {
		return nil
	}
	fields := strings.FieldsFunc(s, func(r rune) bool {
		return r == ',' || r == '\n' || r == ' ' || r == '\t'
	})
	out := make([]string, 0, len(fields))
	for _, f := range fields {
		if f = strings.TrimSpace(f); f != "" {
			out = append(out, f)
		}
	}
	return out
}

func firstNonEmpty(vals ...string) string {
	for _, v := range vals {
		if strings.TrimSpace(v) != "" && strings.TrimSpace(v) != "None" {
			return strings.TrimSpace(v)
		}
	}
	return ""
}

func joinNonEmpty(sep string, vals ...string) string {
	var out []string
	seen := map[string]bool{}
	for _, v := range vals {
		v = strings.TrimSpace(v)
		if v == "" || v == "None" || seen[v] {
			continue
		}
		seen[v] = true
		out = append(out, v)
	}
	return strings.Join(out, sep)
}

// oneLine collapses a multi-line metadata value into a single readable line.
func oneLine(s string) string {
	fields := strings.Fields(strings.ReplaceAll(s, "\n", " "))
	joined := strings.Join(fields, " ")
	const limit = 300
	if len(joined) > limit {
		return joined[:limit] + " …"
	}
	return joined
}

func orNone(s string) string {
	if strings.TrimSpace(s) == "" {
		return "(none)"
	}
	return s
}

// compareVersions orders two version strings the way a human reads them:
// numeric segments compare numerically, and a version with a pre-release
// suffix sorts below the same version without one. It is deliberately simple —
// it only ranks display lists, never decides what to install.
func compareVersions(a, b string) int {
	as, apre := splitVersion(a)
	bs, bpre := splitVersion(b)
	for i := 0; i < len(as) || i < len(bs); i++ {
		var x, y int
		if i < len(as) {
			x = as[i]
		}
		if i < len(bs) {
			y = bs[i]
		}
		if x != y {
			if x < y {
				return -1
			}
			return 1
		}
	}
	// Trailing zero segments do not change the version, so "2.0" and "2.0.0"
	// compare equal rather than by string length.
	switch {
	case apre == bpre:
		return 0
	case apre == "":
		return 1
	case bpre == "":
		return -1
	default:
		return strings.Compare(apre, bpre)
	}
}

// splitVersion returns the leading numeric segments and any trailing
// pre-release/local suffix.
func splitVersion(v string) ([]int, string) {
	v = strings.TrimSpace(v)
	if i := strings.IndexByte(v, ':'); i >= 0 { // strip a Debian epoch
		v = v[i+1:]
	}
	var nums []int
	i := 0
	for i < len(v) {
		if v[i] == '.' {
			i++
			continue
		}
		j := i
		for j < len(v) && v[j] >= '0' && v[j] <= '9' {
			j++
		}
		if j == i {
			break
		}
		n, err := strconv.Atoi(v[i:j])
		if err != nil {
			break
		}
		nums = append(nums, n)
		i = j
		if i < len(v) && v[i] != '.' {
			break
		}
	}
	return nums, v[i:]
}
