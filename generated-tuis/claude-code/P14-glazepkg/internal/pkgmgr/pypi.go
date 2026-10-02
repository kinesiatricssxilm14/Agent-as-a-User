package pkgmgr

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"sync"
	"time"
)

// httpClient is shared so connections are reused across lookups.
var httpClient = &http.Client{Timeout: 60 * time.Second}

const userAgent = "tooln/1.0 (+package management TUI)"

func pypiProjectURL(name string) string {
	return "https://pypi.org/pypi/" + url.PathEscape(strings.TrimSpace(name)) + "/json"
}

// httpJSON fetches url and decodes the response body into out.
func httpJSON(ctx context.Context, log Logger, url string, out any) error {
	body, err := httpGet(ctx, log, url, "application/json")
	if err != nil {
		return err
	}
	if err := json.Unmarshal(body, out); err != nil {
		return fmt.Errorf("decoding %s: %w", url, err)
	}
	return nil
}

func httpGet(ctx context.Context, log Logger, url, accept string) ([]byte, error) {
	resp, err := httpOpen(ctx, log, url, accept)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	// Cap the read so a pathological response cannot exhaust memory.
	return io.ReadAll(io.LimitReader(resp.Body, 8<<20))
}

// httpOpen performs the request and hands back the live response so large
// documents can be streamed rather than buffered.
func httpOpen(ctx context.Context, log Logger, url, accept string) (*http.Response, error) {
	log.log("> GET %s", url)

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", userAgent)
	req.Header.Set("Accept", accept)

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode == http.StatusNotFound {
		resp.Body.Close()
		return nil, notFoundError{url}
	}
	if resp.StatusCode != http.StatusOK {
		resp.Body.Close()
		return nil, fmt.Errorf("%s: %s", url, resp.Status)
	}
	return resp, nil
}

// ----------------------------------------------------------- simple index ----

// PyPI has no keyword search API: the XML-RPC endpoint was retired, and the
// HTML search page is behind a bot challenge that returns a "Client Challenge"
// document rather than results. The supported way to enumerate projects is the
// Simple repository API (PEP 691), so name search reads that index and looks up
// the interesting hits through the per-project JSON API.
const simpleIndexURL = "https://pypi.org/simple/"

const simpleIndexAccept = "application/vnd.pypi.simple.v1+json"

// nameIndex caches the project list. It is a package-level cache because the
// document is large and identical for every caller; a process only needs it
// once.
type nameIndex struct {
	mu      sync.Mutex
	names   []string
	fetched time.Time
	err     error
}

var projectIndex = &nameIndex{}

// indexTTL is how long a fetched index is trusted. New releases appear on PyPI
// constantly, but a session lasting longer than this is rare.
const indexTTL = 30 * time.Minute

// load returns the cached project names, fetching them if needed. A failure is
// cached only for the duration of the call so a transient network problem does
// not disable search for the rest of the session.
func (n *nameIndex) load(ctx context.Context, log Logger) ([]string, error) {
	n.mu.Lock()
	defer n.mu.Unlock()

	if n.names != nil && time.Since(n.fetched) < indexTTL {
		return n.names, nil
	}
	log.log("# fetching the PyPI project index (this happens once per session)")

	resp, err := httpOpen(ctx, log, simpleIndexURL, simpleIndexAccept)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	names, err := decodeSimpleIndex(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("reading the PyPI project index: %w", err)
	}
	n.names = names
	n.fetched = time.Now()
	log.log("# the index lists %d projects", len(names))
	return names, nil
}

// decodeSimpleIndex streams the project names out of a PEP 691 JSON document.
// The document is tens of megabytes, so it is decoded incrementally rather than
// read into memory whole.
func decodeSimpleIndex(r io.Reader) ([]string, error) {
	dec := json.NewDecoder(r)
	for {
		tok, err := dec.Token()
		if err != nil {
			if err == io.EOF {
				return nil, fmt.Errorf("the index has no %q field", "projects")
			}
			return nil, err
		}
		key, ok := tok.(string)
		if !ok || key != "projects" {
			continue
		}
		if _, err := dec.Token(); err != nil { // consume '['
			return nil, err
		}
		names := make([]string, 0, 1<<19)
		for dec.More() {
			var entry struct {
				Name string `json:"name"`
			}
			if err := dec.Decode(&entry); err != nil {
				return nil, err
			}
			if entry.Name != "" {
				names = append(names, entry.Name)
			}
		}
		return names, nil
	}
}

// searchLimit caps how many index matches get a metadata lookup. Each lookup is
// an HTTP request, so this bounds both the time a search takes and the load put
// on PyPI.
const searchLimit = 30

// pypiSearch finds projects whose name matches query, then fills in the version
// and summary of the best matches from the JSON API.
func pypiSearch(ctx context.Context, log Logger, query string) ([]Package, error) {
	names, err := projectIndex.load(ctx, log)
	if err != nil {
		return nil, err
	}

	matches := rankNameMatches(names, query, searchLimit)
	if len(matches) == 0 {
		return nil, nil
	}
	log.log("# %d project name(s) match %q; reading metadata for the closest %d",
		len(matches), query, len(matches))

	// Look the matches up concurrently; a serial pass over 30 names would take
	// far too long to feel like a search.
	out := make([]Package, len(matches))
	var wg sync.WaitGroup
	sem := make(chan struct{}, 8)
	for i, name := range matches {
		wg.Add(1)
		go func(i int, name string) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()

			pkg := Package{Name: name}
			var doc pypiJSON
			if err := httpJSON(ctx, nil, pypiProjectURL(name), &doc); err == nil {
				pkg.Name = firstNonEmpty(doc.Info.Name, name)
				pkg.Version = doc.Info.Version
				pkg.Summary = doc.Info.Summary
			}
			out[i] = pkg
		}(i, name)
	}
	wg.Wait()

	// Preserve the ranking, dropping anything the lookup could not name.
	res := make([]Package, 0, len(out))
	for _, p := range out {
		if p.Name != "" {
			res = append(res, p)
		}
	}
	return res, nil
}

// rankNameMatches finds the project names most relevant to query. Exact matches
// come first, then prefixes, then names that contain the query as a word, then
// any other substring hit; ties break towards shorter names, which are almost
// always the canonical project rather than a fork or plugin.
func rankNameMatches(names []string, query string, limit int) []string {
	q := strings.ToLower(strings.TrimSpace(query))
	if q == "" {
		return nil
	}
	qNorm := normalizeName(q)

	type hit struct {
		name string
		rank int
	}
	var hits []hit
	for _, name := range names {
		lower := strings.ToLower(name)
		var rank int
		switch {
		case lower == q || normalizeName(name) == qNorm:
			rank = 0
		case strings.HasPrefix(lower, q):
			rank = 1
		case isWordMatch(lower, q):
			rank = 2
		case strings.Contains(lower, q):
			rank = 3
		default:
			continue
		}
		hits = append(hits, hit{name: name, rank: rank})
	}
	sort.Slice(hits, func(i, j int) bool {
		if hits[i].rank != hits[j].rank {
			return hits[i].rank < hits[j].rank
		}
		if len(hits[i].name) != len(hits[j].name) {
			return len(hits[i].name) < len(hits[j].name)
		}
		return hits[i].name < hits[j].name
	})
	if len(hits) > limit {
		hits = hits[:limit]
	}
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		out = append(out, h.name)
	}
	return out
}

// isWordMatch reports whether query appears in name delimited by the separators
// projects use, so "flask" matches "flask-login" and "django-flask" but the hit
// ranks above an incidental substring such as "microflasker".
func isWordMatch(name, query string) bool {
	idx := 0
	for {
		i := strings.Index(name[idx:], query)
		if i < 0 {
			return false
		}
		i += idx
		beforeOK := i == 0 || isNameSep(name[i-1])
		end := i + len(query)
		afterOK := end == len(name) || isNameSep(name[end])
		if beforeOK && afterOK {
			return true
		}
		idx = i + 1
		if idx >= len(name) {
			return false
		}
	}
}

func isNameSep(b byte) bool { return b == '-' || b == '_' || b == '.' }
