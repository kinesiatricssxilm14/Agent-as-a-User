package docker

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"
)

const defaultSocket = "/var/run/docker.sock"

// Client is a minimal Docker Engine API client over a Unix socket or TCP.
type Client struct {
	http    *http.Client
	baseURL string
	socket  string
}

// New builds a client from the environment. It honours DOCKER_HOST, which may
// be a unix:// path, a tcp:// or http(s):// address, or a bare socket path.
func New() *Client {
	socket := defaultSocket
	baseURL := "http://docker"
	useUnix := true

	host := strings.TrimSpace(os.Getenv("DOCKER_HOST"))
	switch {
	case host == "":
		// Use the default local socket.
	case strings.HasPrefix(host, "unix://"):
		socket = strings.TrimPrefix(host, "unix://")
	case strings.HasPrefix(host, "tcp://"):
		baseURL = "http://" + strings.TrimPrefix(host, "tcp://")
		useUnix = false
	case strings.HasPrefix(host, "http://"), strings.HasPrefix(host, "https://"):
		baseURL = host
		useUnix = false
	default:
		// A bare path is treated as a Unix socket path.
		socket = host
	}

	transport := &http.Transport{
		MaxIdleConns:          8,
		IdleConnTimeout:       30 * time.Second,
		TLSHandshakeTimeout:   10 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
	}
	if useUnix {
		transport.DialContext = func(ctx context.Context, _, _ string) (net.Conn, error) {
			d := net.Dialer{Timeout: 15 * time.Second}
			return d.DialContext(ctx, "unix", socket)
		}
	}

	return &Client{
		http:    &http.Client{Transport: transport},
		baseURL: baseURL,
		socket:  socket,
	}
}

// Socket returns the configured Unix socket path (for diagnostics).
func (c *Client) Socket() string { return c.socket }

// do performs a request and decodes a JSON response into out (when non-nil).
func (c *Client) do(ctx context.Context, method, path string, query url.Values, body io.Reader, out interface{}) error {
	u := c.baseURL + path
	if len(query) > 0 {
		u += "?" + query.Encode()
	}
	req, err := http.NewRequestWithContext(ctx, method, u, body)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		data, _ := io.ReadAll(io.LimitReader(resp.Body, 8192))
		msg := strings.TrimSpace(string(data))
		if msg != "" {
			return fmt.Errorf("%s %s: %s (%s)", method, path, resp.Status, msg)
		}
		return fmt.Errorf("%s %s: %s", method, path, resp.Status)
	}
	if out != nil {
		if err := json.NewDecoder(resp.Body).Decode(out); err != nil {
			return fmt.Errorf("decode response for %s: %w", path, err)
		}
	}
	return nil
}
