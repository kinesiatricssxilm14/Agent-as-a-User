package docker

import (
	"bufio"
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Client is the set of read operations toolm needs from a Docker engine. It is
// an interface so the HTTP API client and the docker CLI fallback can be used
// interchangeably.
type Client interface {
	// Endpoint describes where this client talks to, for display purposes.
	Endpoint() string
	// Version returns the engine version string, if the endpoint exposes it.
	Version(ctx context.Context) (string, error)
	Containers(ctx context.Context) ([]Container, error)
	ContainerInspect(ctx context.Context, id string) (*ContainerDetails, error)
	ContainerLogs(ctx context.Context, id string) (string, error)
	Images(ctx context.Context) ([]Image, error)
	ImageInspect(ctx context.Context, id string) (*ImageDetails, error)
	Networks(ctx context.Context) ([]Network, error)
	NetworkInspect(ctx context.Context, id string) (*Network, error)
	Volumes(ctx context.Context) ([]Volume, error)
	VolumeInspect(ctx context.Context, name string) (*Volume, error)
}

// APIClient talks to a Docker daemon (or an API compatible mock) over a unix
// socket, a named pipe or TCP.
type APIClient struct {
	http     *http.Client
	baseURL  string // scheme://host prefix used for requests
	endpoint string // human readable description of the connection
}

// DefaultSocketPaths lists the socket locations probed when DOCKER_HOST is not
// set. The first one that exists wins.
var DefaultSocketPaths = []string{
	"/var/run/docker.sock",
	"/run/docker.sock",
	"/var/run/docker.sock.mock",
	"/tmp/docker.sock",
}

// NewClient builds the client used by the TUI. It honours DOCKER_HOST and
// falls back to the well known local socket paths. When no API endpoint can be
// reached it falls back to driving the docker CLI, so the tool still works on
// hosts where only the client binary is configured.
func NewClient(ctx context.Context) (Client, error) {
	var attempts []string

	for _, host := range candidateHosts() {
		c, err := NewAPIClient(host)
		if err != nil {
			attempts = append(attempts, fmt.Sprintf("%s: %v", host, err))
			continue
		}
		pingCtx, cancel := context.WithTimeout(ctx, 3*time.Second)
		err = c.ping(pingCtx)
		cancel()
		if err == nil {
			return c, nil
		}
		attempts = append(attempts, fmt.Sprintf("%s: %v", host, err))
	}

	if cli, err := NewCLIClient(ctx); err == nil {
		return cli, nil
	} else {
		attempts = append(attempts, fmt.Sprintf("docker CLI: %v", err))
	}

	return nil, fmt.Errorf("cannot reach a Docker endpoint:\n  %s", strings.Join(attempts, "\n  "))
}

// candidateHosts returns the DOCKER_HOST style endpoints to probe, in order.
func candidateHosts() []string {
	var hosts []string
	seen := map[string]bool{}
	add := func(h string) {
		if h == "" || seen[h] {
			return
		}
		seen[h] = true
		hosts = append(hosts, h)
	}

	add(strings.TrimSpace(os.Getenv("DOCKER_HOST")))
	if ctxDir := strings.TrimSpace(os.Getenv("DOCKER_SOCKET")); ctxDir != "" {
		add("unix://" + ctxDir)
	}
	for _, p := range DefaultSocketPaths {
		if fi, err := os.Stat(p); err == nil && fi.Mode()&os.ModeDir == 0 {
			add("unix://" + p)
		}
	}
	// Probe the canonical path last even when the stat above failed: the mock
	// may create the socket slightly later than toolm starts.
	add("unix:///var/run/docker.sock")
	return hosts
}

// NewAPIClient creates a client for a single DOCKER_HOST style endpoint.
func NewAPIClient(host string) (*APIClient, error) {
	if host == "" {
		return nil, errors.New("empty host")
	}

	scheme, addr := splitHost(host)
	switch scheme {
	case "unix", "npipe":
		if addr == "" {
			return nil, errors.New("missing socket path")
		}
		sock := addr
		dial := func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", sock)
		}
		return &APIClient{
			http:     &http.Client{Transport: &http.Transport{DialContext: dial}, Timeout: 30 * time.Second},
			baseURL:  "http://docker",
			endpoint: "unix://" + sock,
		}, nil
	case "tcp", "http", "https":
		target := scheme + "://" + addr
		if scheme == "tcp" {
			target = "http://" + addr
		}
		u, err := url.Parse(target)
		if err != nil {
			return nil, err
		}
		if u.Host == "" {
			return nil, fmt.Errorf("invalid host %q", host)
		}
		return &APIClient{
			http:     &http.Client{Timeout: 30 * time.Second},
			baseURL:  u.Scheme + "://" + u.Host,
			endpoint: host,
		}, nil
	default:
		// A bare path is treated as a socket.
		if filepath.IsAbs(host) {
			return NewAPIClient("unix://" + host)
		}
		return nil, fmt.Errorf("unsupported Docker host scheme %q", scheme)
	}
}

// splitHost splits "unix:///var/run/docker.sock" into ("unix", "/var/run/docker.sock").
func splitHost(host string) (scheme, addr string) {
	if i := strings.Index(host, "://"); i >= 0 {
		return host[:i], host[i+3:]
	}
	return "", host
}

// Endpoint implements Client.
func (c *APIClient) Endpoint() string { return c.endpoint }

// ping verifies the endpoint answers. It tries the cheap _ping route first and
// accepts /version as an alternative, since minimal mocks often skip _ping.
func (c *APIClient) ping(ctx context.Context) error {
	if _, err := c.get(ctx, "/_ping"); err == nil {
		return nil
	}
	if _, err := c.get(ctx, "/version"); err == nil {
		return nil
	}
	// Last resort: any endpoint that answers means the socket is usable.
	_, err := c.get(ctx, "/containers/json?all=1")
	return err
}

// Version implements Client.
func (c *APIClient) Version(ctx context.Context) (string, error) {
	body, err := c.get(ctx, "/version")
	if err != nil {
		return "", err
	}
	var v struct {
		Version    string `json:"Version"`
		APIVersion string `json:"ApiVersion"`
	}
	if err := json.Unmarshal(body, &v); err != nil {
		return "", err
	}
	switch {
	case v.Version != "" && v.APIVersion != "":
		return fmt.Sprintf("%s (API %s)", v.Version, v.APIVersion), nil
	case v.Version != "":
		return v.Version, nil
	case v.APIVersion != "":
		return "API " + v.APIVersion, nil
	}
	return "", errors.New("no version reported")
}

// get performs a GET request against the Docker API and returns the raw body.
func (c *APIClient) get(ctx context.Context, path string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+path, nil)
	if err != nil {
		return nil, err
	}
	req.Host = "docker"
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, unwrapURLError(err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, apiError(path, resp.StatusCode, body)
	}
	return body, nil
}

// unwrapURLError strips the noisy *url.Error wrapper so messages stay short.
func unwrapURLError(err error) error {
	var uerr *url.Error
	if errors.As(err, &uerr) && uerr.Err != nil {
		return uerr.Err
	}
	return err
}

// apiError turns a non-2xx response into a readable error.
func apiError(path string, status int, body []byte) error {
	msg := strings.TrimSpace(string(body))
	var payload struct {
		Message string `json:"message"`
	}
	if json.Unmarshal(body, &payload) == nil && payload.Message != "" {
		msg = payload.Message
	}
	if len(msg) > 200 {
		msg = msg[:200] + "..."
	}
	if msg == "" {
		msg = http.StatusText(status)
	}
	return fmt.Errorf("GET %s: %d %s", path, status, msg)
}

// Containers implements Client.
func (c *APIClient) Containers(ctx context.Context) ([]Container, error) {
	body, err := c.get(ctx, "/containers/json?all=1")
	if err != nil {
		return nil, err
	}
	var list []Container
	if err := json.Unmarshal(body, &list); err != nil {
		return nil, fmt.Errorf("decoding container list: %w", err)
	}
	return list, nil
}

// ContainerInspect implements Client.
func (c *APIClient) ContainerInspect(ctx context.Context, id string) (*ContainerDetails, error) {
	body, err := c.get(ctx, "/containers/"+url.PathEscape(id)+"/json")
	if err != nil {
		return nil, err
	}
	var d ContainerDetails
	if err := json.Unmarshal(body, &d); err != nil {
		return nil, fmt.Errorf("decoding container details: %w", err)
	}
	return &d, nil
}

// ContainerLogs implements Client. It requests the full log history and
// de-multiplexes the stream when the daemon uses the framed format.
func (c *APIClient) ContainerLogs(ctx context.Context, id string) (string, error) {
	esc := url.PathEscape(id)
	paths := []string{
		"/containers/" + esc + "/logs?stdout=1&stderr=1&tail=all&timestamps=0",
		"/containers/" + esc + "/logs?stdout=1&stderr=1",
		"/containers/" + esc + "/logs",
	}
	var lastErr error
	for _, p := range paths {
		body, err := c.get(ctx, p)
		if err != nil {
			lastErr = err
			continue
		}
		return string(DemuxLogs(body)), nil
	}
	return "", lastErr
}

// Images implements Client.
func (c *APIClient) Images(ctx context.Context) ([]Image, error) {
	body, err := c.get(ctx, "/images/json?all=0")
	if err != nil {
		// Older/mock endpoints may reject the query string.
		body, err = c.get(ctx, "/images/json")
		if err != nil {
			return nil, err
		}
	}
	var list []Image
	if err := json.Unmarshal(body, &list); err != nil {
		return nil, fmt.Errorf("decoding image list: %w", err)
	}
	return list, nil
}

// ImageInspect implements Client.
func (c *APIClient) ImageInspect(ctx context.Context, id string) (*ImageDetails, error) {
	body, err := c.get(ctx, "/images/"+escapeImageRef(id)+"/json")
	if err != nil {
		return nil, err
	}
	var d ImageDetails
	if err := json.Unmarshal(body, &d); err != nil {
		return nil, fmt.Errorf("decoding image details: %w", err)
	}
	return &d, nil
}

// escapeImageRef escapes an image reference for use in a URL path while
// keeping the slashes that separate registry and repository components.
func escapeImageRef(ref string) string {
	parts := strings.Split(ref, "/")
	for i, p := range parts {
		parts[i] = url.PathEscape(p)
	}
	return strings.Join(parts, "/")
}

// Networks implements Client.
func (c *APIClient) Networks(ctx context.Context) ([]Network, error) {
	body, err := c.get(ctx, "/networks")
	if err != nil {
		return nil, err
	}
	var list []Network
	if err := json.Unmarshal(body, &list); err != nil {
		return nil, fmt.Errorf("decoding network list: %w", err)
	}
	return list, nil
}

// NetworkInspect implements Client.
func (c *APIClient) NetworkInspect(ctx context.Context, id string) (*Network, error) {
	body, err := c.get(ctx, "/networks/"+url.PathEscape(id))
	if err != nil {
		return nil, err
	}
	var n Network
	if err := json.Unmarshal(body, &n); err != nil {
		return nil, fmt.Errorf("decoding network details: %w", err)
	}
	return &n, nil
}

// Volumes implements Client. It accepts both the documented object response
// and a bare array, which some mocks return.
func (c *APIClient) Volumes(ctx context.Context) ([]Volume, error) {
	body, err := c.get(ctx, "/volumes")
	if err != nil {
		return nil, err
	}
	trimmed := bytes.TrimSpace(body)
	if len(trimmed) > 0 && trimmed[0] == '[' {
		var list []Volume
		if err := json.Unmarshal(trimmed, &list); err != nil {
			return nil, fmt.Errorf("decoding volume list: %w", err)
		}
		return list, nil
	}
	var payload struct {
		Volumes  []Volume `json:"Volumes"`
		Warnings []string `json:"Warnings"`
	}
	if err := json.Unmarshal(trimmed, &payload); err != nil {
		return nil, fmt.Errorf("decoding volume list: %w", err)
	}
	return payload.Volumes, nil
}

// VolumeInspect implements Client.
func (c *APIClient) VolumeInspect(ctx context.Context, name string) (*Volume, error) {
	body, err := c.get(ctx, "/volumes/"+url.PathEscape(name))
	if err != nil {
		return nil, err
	}
	var v Volume
	if err := json.Unmarshal(body, &v); err != nil {
		return nil, fmt.Errorf("decoding volume details: %w", err)
	}
	return &v, nil
}

// DemuxLogs converts a container log payload into plain text. Docker frames
// non-TTY logs as repeated 8 byte headers (stream, 3 zero bytes, big endian
// length) followed by the payload; TTY logs and simple mocks return raw bytes.
func DemuxLogs(data []byte) []byte {
	if !looksFramed(data) {
		return data
	}
	var out bytes.Buffer
	for len(data) >= 8 {
		size := int(binary.BigEndian.Uint32(data[4:8]))
		data = data[8:]
		if size > len(data) {
			size = len(data)
		}
		out.Write(data[:size])
		data = data[size:]
	}
	// Trailing bytes that do not form a frame are kept verbatim.
	out.Write(data)
	return out.Bytes()
}

// looksFramed reports whether data starts with a plausible Docker stream
// frame. It walks the whole payload so a text log that happens to begin with a
// low byte is not mistaken for a framed stream.
func looksFramed(data []byte) bool {
	if len(data) < 8 {
		return false
	}
	frames := 0
	for len(data) > 0 {
		if len(data) < 8 {
			return false
		}
		if data[0] > 2 || data[1] != 0 || data[2] != 0 || data[3] != 0 {
			return false
		}
		size := int(binary.BigEndian.Uint32(data[4:8]))
		if size < 0 || size > len(data)-8 {
			return false
		}
		data = data[8+size:]
		frames++
	}
	return frames > 0
}

// SplitLogLines turns a log blob into display lines, normalising newlines and
// carriage returns and expanding tabs so the viewport renders predictably.
func SplitLogLines(raw string) []string {
	raw = strings.ReplaceAll(raw, "\r\n", "\n")
	raw = strings.ReplaceAll(raw, "\r", "\n")
	raw = strings.TrimSuffix(raw, "\n")
	if raw == "" {
		return nil
	}
	sc := bufio.NewScanner(strings.NewReader(raw))
	sc.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
	var lines []string
	for sc.Scan() {
		lines = append(lines, strings.ReplaceAll(sc.Text(), "\t", "    "))
	}
	if err := sc.Err(); err != nil {
		lines = append(lines, strings.Split(raw, "\n")...)
	}
	return lines
}
