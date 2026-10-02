package pkgmgr

import (
	"reflect"
	"testing"
)

func TestParseRFC822PipShow(t *testing.T) {
	// Real `pip show --verbose` output: note the fields with an empty value and
	// a trailing space, and the indented continuation of Description. Treating
	// "Home-page: " as a continuation of the line above corrupts Summary, which
	// is what the isIndented check exists to prevent.
	out := "Name: agent-tui\n" +
		"Version: 0.1.0\n" +
		"Summary: A CLI tool for LLM Agents\n" +
		"Home-page: \n" +
		"Author: \n" +
		"Author-email: anonymous@example.invalid\n" +
		"License: MIT\n" +
		"Location: /usr/lib/python3/dist-packages\n" +
		"Requires: ansi2html, click, rich\n" +
		"Required-by: \n" +
		"Description: line one\n" +
		"        line two\n" +
		"---\n" +
		"Name: click\n" +
		"Version: 8.1.7\n" +
		"Requires: \n" +
		"Required-by: agent-tui\n"

	blocks := parseRFC822(out)
	if len(blocks) != 2 {
		t.Fatalf("got %d records, want 2", len(blocks))
	}

	first := blocks[0]
	checks := map[string]string{
		"Name":      "agent-tui",
		"Version":   "0.1.0",
		"Summary":   "A CLI tool for LLM Agents",
		"Home-page": "",
		"License":   "MIT",
	}
	for key, want := range checks {
		if got := first.get(key); got != want {
			t.Errorf("record 1 %s = %q, want %q", key, got, want)
		}
	}
	if got, want := splitList(first.get("Requires")), []string{"ansi2html", "click", "rich"}; !reflect.DeepEqual(got, want) {
		t.Errorf("Requires = %v, want %v", got, want)
	}
	if got := splitList(first.get("Required-by")); len(got) != 0 {
		t.Errorf("Required-by = %v, want empty", got)
	}
	if got, want := oneLine(first.get("Description")), "line one line two"; got != want {
		t.Errorf("Description = %q, want %q", got, want)
	}
	if got, want := blocks[1].get("Name"), "click"; got != want {
		t.Errorf("record 2 Name = %q, want %q", got, want)
	}
}

func TestParseRFC822AptCacheShow(t *testing.T) {
	out := "Package: coreutils\n" +
		"Version: 9.1-1\n" +
		"Installed-Size: 18000\n" +
		"Depends: libacl1 (>= 2.2.23), libattr1 (>= 1:2.4.44), libc6 (>= 2.34)\n" +
		"Section: utils\n" +
		"Description-en: GNU core utilities\n" +
		" This package contains the basic file, shell and text\n" +
		" manipulation utilities.\n"

	blocks := parseRFC822(out)
	if len(blocks) != 1 {
		t.Fatalf("got %d records, want 1", len(blocks))
	}
	b := blocks[0]
	if got, want := b.get("Package"), "coreutils"; got != want {
		t.Errorf("Package = %q, want %q", got, want)
	}
	if got, want := aptDepNames(b.get("Depends")), []string{"libacl1", "libattr1", "libc6"}; !reflect.DeepEqual(got, want) {
		t.Errorf("aptDepNames = %v, want %v", got, want)
	}
	if got := splitList2(b.get("Depends")); len(got) != 3 || got[0] != "libacl1 (>= 2.2.23)" {
		t.Errorf("splitList2 = %v, want the version constraints kept", got)
	}
	if got, want := humanKB("18000"), "17.6 MB"; got != want {
		t.Errorf("humanKB = %q, want %q", got, want)
	}
}

func TestNormalizeName(t *testing.T) {
	// PEP 503 normalization: these all name the same project.
	for _, in := range []string{"Zope.Interface", "zope-interface", "zope_interface", "ZOPE..INTERFACE"} {
		if got, want := normalizeName(in), "zope-interface"; got != want {
			t.Errorf("normalizeName(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestBaseRequirement(t *testing.T) {
	cases := []struct {
		in   string
		want string
		ok   bool
	}{
		{"click>=8.1.3", "click", true},
		{"Jinja2>=3.1.2", "Jinja2", true},
		{"importlib-metadata>=3.6.0; python_version < \"3.10\"", "importlib-metadata", true},
		{"requests[socks]", "requests", true},
		{"pytest; extra == 'test'", "", false}, // only needed for an extra
		{"sphinx ; extra == \"docs\"", "", false},
		{"blinker>=1.6.2", "blinker", true},
		{"", "", false},
	}
	for _, tc := range cases {
		got, ok := baseRequirement(tc.in)
		if got != tc.want || ok != tc.ok {
			t.Errorf("baseRequirement(%q) = (%q,%v), want (%q,%v)", tc.in, got, ok, tc.want, tc.ok)
		}
	}
}

func TestValidateSpec(t *testing.T) {
	valid := []string{"flask", "flask==3.0.0", "requests>=2.0,<3.0", "requests[socks]", "ruamel.yaml", "a_b-c.d"}
	for _, s := range valid {
		if err := validateSpec(s); err != nil {
			t.Errorf("validateSpec(%q) = %v, want nil", s, err)
		}
	}
	// Shell metacharacters and flags must be refused: these strings become argv
	// entries, so anything that could be reinterpreted is rejected up front.
	// Note that ">" and "<" are allowed, since they are PEP 508 comparison
	// operators — the spec never reaches a shell, so they are safe here.
	invalid := []string{"flask; rm -rf /", "flask && echo", "$(whoami)", "`id`", "--upgrade", "-r req.txt", "a|b", "a/b", "a&b"}
	for _, s := range invalid {
		if err := validateSpec(s); err == nil {
			t.Errorf("validateSpec(%q) = nil, want an error", s)
		}
	}
}

func TestValidateAptName(t *testing.T) {
	valid := []string{"coreutils", "libc6", "gcc-12-base", "libc6:arm64", "python3.11", "sl=3.03-17build3"}
	for _, s := range valid {
		if err := validateAptName(s); err != nil {
			t.Errorf("validateAptName(%q) = %v, want nil", s, err)
		}
	}
	invalid := []string{"", "sl; reboot", "-sl", "$(id)", "a b", "a&&b", "../etc/passwd"}
	for _, s := range invalid {
		if err := validateAptName(s); err == nil {
			t.Errorf("validateAptName(%q) = nil, want an error", s)
		}
	}
}

func TestCompareVersions(t *testing.T) {
	cases := []struct {
		a, b string
		want int
	}{
		{"1.2.3", "1.2.4", -1},
		{"1.10.0", "1.9.0", 1},
		{"2.0", "2.0.0", 0},
		{"3.1.0", "3.1.0rc1", 1}, // a release outranks its own pre-release
		{"1:2.38.1-5", "1:2.38.1-4", 1},
	}
	for _, tc := range cases {
		got := compareVersions(tc.a, tc.b)
		if (got < 0) != (tc.want < 0) || (got > 0) != (tc.want > 0) {
			t.Errorf("compareVersions(%q,%q) = %d, want sign %d", tc.a, tc.b, got, tc.want)
		}
	}
}

func TestRankNameMatches(t *testing.T) {
	names := []string{
		"microflasker", "flask-login", "Flask", "flasky", "django-flask",
		"flask", "unrelated", "FLASK-CORS",
	}
	got := rankNameMatches(names, "flask", 6)
	if len(got) == 0 {
		t.Fatal("no matches")
	}
	// An exact match must come first, whatever its capitalisation.
	if lower := normalizeName(got[0]); lower != "flask" {
		t.Errorf("first match = %q, want an exact 'flask'", got[0])
	}
	// An incidental substring must rank below the delimited matches.
	posOf := func(n string) int {
		for i, g := range got {
			if g == n {
				return i
			}
		}
		return -1
	}
	if p, q := posOf("flask-login"), posOf("microflasker"); p >= 0 && q >= 0 && p > q {
		t.Errorf("flask-login (%d) should rank above microflasker (%d)", p, q)
	}
	if posOf("unrelated") != -1 {
		t.Error("unrelated should not match")
	}
}

func TestIsWordMatch(t *testing.T) {
	cases := []struct {
		name, query string
		want        bool
	}{
		{"flask-login", "flask", true},
		{"django-flask", "flask", true},
		{"flask_sqlalchemy", "flask", true},
		{"microflasker", "flask", false},
		{"flask", "flask", true},
		{"a.flask.b", "flask", true},
	}
	for _, tc := range cases {
		if got := isWordMatch(tc.name, tc.query); got != tc.want {
			t.Errorf("isWordMatch(%q,%q) = %v, want %v", tc.name, tc.query, got, tc.want)
		}
	}
}

func TestCommandDisplay(t *testing.T) {
	c := Command{Name: "/usr/bin/python3", Args: []string{"-m", "pip", "install", "flask==3.0.0"}}
	if got, want := c.Display(), "/usr/bin/python3 -m pip install flask==3.0.0"; got != want {
		t.Errorf("Display() = %q, want %q", got, want)
	}
	// An argument with a space is quoted so the displayed line is copy-pasteable.
	c2 := Command{Name: "apt-get", Args: []string{"install", "a b"}}
	if got, want := c2.Display(), `apt-get install "a b"`; got != want {
		t.Errorf("Display() = %q, want %q", got, want)
	}
}

func TestJSONSlice(t *testing.T) {
	// pip occasionally prints a warning on stdout before the JSON document.
	in := "WARNING: something happened\n[{\"name\":\"x\",\"version\":\"1\"}]\n"
	if got, want := jsonSlice(in), `[{"name":"x","version":"1"}]`; got != want {
		t.Errorf("jsonSlice = %q, want %q", got, want)
	}
}

func TestUpgradableParsing(t *testing.T) {
	out := "Listing...\n" +
		"libc6/stable 2.36-9+deb12u10 arm64 [upgradable from: 2.36-9+deb12u9]\n" +
		"tzdata/stable-updates 2025b-0+deb12u1 all [upgradable from: 2024a-0+deb12u1]\n"
	found := map[string]string{}
	for _, line := range splitLines(out) {
		if m := upgradableRe.FindStringSubmatch(line); m != nil {
			found[m[1]] = m[2]
		}
	}
	if got, want := found["libc6"], "2.36-9+deb12u10"; got != want {
		t.Errorf("libc6 candidate = %q, want %q", got, want)
	}
	if len(found) != 2 {
		t.Errorf("parsed %d entries, want 2", len(found))
	}
}

// splitLines is a test helper mirroring how Outdated walks apt's output.
func splitLines(s string) []string {
	var out []string
	start := 0
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			out = append(out, s[start:i])
			start = i + 1
		}
	}
	if start < len(s) {
		out = append(out, s[start:])
	}
	return out
}
