package manager

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/tooln/tooln/internal/pkgs"
	"golang.org/x/net/html"
)

const pypiBase = "https://pypi.org"

// SearchPypi searches the Python Package Index for a name or keyword. It uses
// the human search page (parsed as HTML) for keyword matches and falls back to
// an exact-name lookup against the JSON API when the search page yields nothing.
func SearchPypi(ctx context.Context, query string) ([]pkgs.SearchResult, error) {
	q := strings.TrimSpace(query)
	if q == "" {
		return nil, nil
	}

	client := &http.Client{Timeout: 20 * time.Second}

	// 1) Keyword search over the search page.
	results, err := pypiSearchHTML(ctx, client, q)
	if err == nil && len(results) > 0 {
		return results, nil
	}

	// 2) Exact-name fallback.
	if res, ok := pypiExact(ctx, client, q); ok {
		return []pkgs.SearchResult{res}, nil
	}
	if err != nil {
		return nil, err
	}
	return nil, nil
}

// pypiExact queries https://pypi.org/pypi/<name>/json for a single project.
func pypiExact(ctx context.Context, client *http.Client, name string) (pkgs.SearchResult, bool) {
	u := pypiBase + "/pypi/" + url.PathEscape(name) + "/json"
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return pkgs.SearchResult{}, false
	}
	req.Header.Set("User-Agent", "tooln/1.0")
	resp, err := client.Do(req)
	if err != nil {
		return pkgs.SearchResult{}, false
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return pkgs.SearchResult{}, false
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return pkgs.SearchResult{}, false
	}
	var doc struct {
		Info struct {
			Name         string   `json:"name"`
			Version      string   `json:"version"`
			Summary      string   `json:"summary"`
			RequiresDist []string `json:"requires_dist"`
		} `json:"info"`
	}
	if err := json.Unmarshal(body, &doc); err != nil || doc.Info.Name == "" {
		return pkgs.SearchResult{}, false
	}
	return pkgs.SearchResult{
		Name:    doc.Info.Name,
		Version: doc.Info.Version,
		Summary: strings.TrimSpace(doc.Info.Summary),
	}, true
}

// pypiSearchHTML parses https://pypi.org/search/?q=<query> and extracts the
// package snippets rendered by Warehouse.
func pypiSearchHTML(ctx context.Context, client *http.Client, query string) ([]pkgs.SearchResult, error) {
	u := pypiBase + "/search/?" + url.Values{"q": []string{query}}.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "tooln/1.0")
	req.Header.Set("Accept", "text/html")
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("PyPI search returned status %d", resp.StatusCode)
	}
	doc, err := html.Parse(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return nil, err
	}
	return extractPackageSnippets(doc), nil
}

// extractPackageSnippets walks the parsed HTML looking for
// <a class="package-snippet"> elements and reads their name, version and
// description child nodes.
func extractPackageSnippets(n *html.Node) []pkgs.SearchResult {
	var results []pkgs.SearchResult
	var walk func(*html.Node)
	walk = func(node *html.Node) {
		if node.Type == html.ElementNode && node.Data == "a" && hasClass(node, "package-snippet") {
			r := pkgs.SearchResult{}
			collect := func(c *html.Node) {
				switch {
				case c.Type == html.ElementNode && hasClass(c, "package-snippet__name"):
					r.Name = strings.TrimSpace(textContent(c))
				case c.Type == html.ElementNode && hasClass(c, "package-snippet__version"):
					r.Version = strings.TrimSpace(textContent(c))
				case c.Type == html.ElementNode && hasClass(c, "package-snippet__description"):
					r.Summary = strings.TrimSpace(textContent(c))
				}
			}
			walkChildren(node, collect)
			if r.Name != "" {
				results = append(results, r)
			}
		}
		for c := node.FirstChild; c != nil; c = c.NextSibling {
			walk(c)
		}
	}
	walk(n)
	return results
}

// walkChildren visits every descendant of node, invoking fn on each element.
func walkChildren(node *html.Node, fn func(*html.Node)) {
	for c := node.FirstChild; c != nil; c = c.NextSibling {
		fn(c)
		walkChildren(c, fn)
	}
}

// hasClass reports whether node's class attribute contains the target class.
func hasClass(node *html.Node, target string) bool {
	for _, a := range node.Attr {
		if a.Key == "class" {
			for _, cls := range strings.Fields(a.Val) {
				if cls == target {
					return true
				}
			}
		}
	}
	return false
}

// textContent returns the concatenated text of a node's descendants.
func textContent(node *html.Node) string {
	var sb strings.Builder
	var collect func(*html.Node)
	collect = func(n *html.Node) {
		if n.Type == html.TextNode {
			sb.WriteString(n.Data)
		}
		for c := n.FirstChild; c != nil; c = c.NextSibling {
			collect(c)
		}
	}
	collect(node)
	return sb.String()
}
