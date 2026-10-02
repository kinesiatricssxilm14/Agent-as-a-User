package dockerapi

import (
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
	"path"
	"strings"
	"time"
)

// Client is a small Docker Engine API client. It intentionally uses the HTTP
// API directly, so it also works with benchmark/mock Unix sockets without the
// Docker CLI or daemon SDK being present.
type Client struct {
	http    *http.Client
	baseURL string
	target  string
}

func NewFromEnv() (*Client, error) {
	host := strings.TrimSpace(os.Getenv("DOCKER_HOST"))
	if host == "" {
		host = "unix:///var/run/docker.sock"
	}

	if strings.HasPrefix(host, "unix://") {
		socket := strings.TrimPrefix(host, "unix://")
		if socket == "" {
			return nil, errors.New("DOCKER_HOST has an empty Unix socket path")
		}
		transport := &http.Transport{
			DisableCompression: true,
			DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
				return (&net.Dialer{Timeout: 5 * time.Second}).DialContext(ctx, "unix", socket)
			},
		}
		return &Client{
			http:    &http.Client{Transport: transport},
			baseURL: "http://docker",
			target:  socket,
		}, nil
	}

	if strings.HasPrefix(host, "tcp://") {
		host = "http://" + strings.TrimPrefix(host, "tcp://")
	}
	parsed, err := url.Parse(host)
	if err != nil || (parsed.Scheme != "http" && parsed.Scheme != "https") || parsed.Host == "" {
		return nil, fmt.Errorf("unsupported DOCKER_HOST %q (use unix://, tcp://, http://, or https://)", host)
	}
	return &Client{
		http:    &http.Client{Transport: &http.Transport{DisableCompression: true}},
		baseURL: strings.TrimRight(host, "/"),
		target:  host,
	}, nil
}

func (c *Client) Target() string { return c.target }

func (c *Client) Containers(ctx context.Context) ([]Container, error) {
	var out []Container
	err := c.getJSON(ctx, "/containers/json?all=1", &out)
	return out, err
}

func (c *Client) Container(ctx context.Context, id string) (ContainerInspect, error) {
	var out ContainerInspect
	err := c.getJSON(ctx, resourcePath("containers", id, "json"), &out)
	return out, err
}

func (c *Client) ContainerLogs(ctx context.Context, id string) (string, error) {
	endpoint := resourcePath("containers", id, "logs") + "?stdout=1&stderr=1&timestamps=0&tail=all"
	body, contentType, err := c.getBytes(ctx, endpoint)
	if err != nil {
		return "", err
	}
	if strings.Contains(contentType, "application/vnd.docker.raw-stream") {
		if decoded, ok := decodeDockerStream(body); ok {
			return decoded, nil
		}
	}
	// Some mock APIs omit the content type but still return multiplexed frames.
	if decoded, ok := decodeDockerStream(body); ok {
		return decoded, nil
	}
	return string(body), nil
}

func (c *Client) Images(ctx context.Context) ([]Image, error) {
	var out []Image
	err := c.getJSON(ctx, "/images/json?all=1", &out)
	return out, err
}

func (c *Client) Networks(ctx context.Context) ([]Network, error) {
	var out []Network
	err := c.getJSON(ctx, "/networks", &out)
	return out, err
}

func (c *Client) Volumes(ctx context.Context) ([]Volume, []string, error) {
	var out volumeListResponse
	err := c.getJSON(ctx, "/volumes", &out)
	return out.Volumes, out.Warnings, err
}

func (c *Client) Volume(ctx context.Context, name string) (Volume, error) {
	var out Volume
	err := c.getJSON(ctx, resourcePath("volumes", name), &out)
	return out, err
}

func resourcePath(parts ...string) string {
	escaped := make([]string, len(parts))
	for i, part := range parts {
		escaped[i] = url.PathEscape(part)
	}
	return "/" + path.Join(escaped...)
}

func (c *Client) getJSON(ctx context.Context, endpoint string, dst any) error {
	body, _, err := c.getBytes(ctx, endpoint)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(body, dst); err != nil {
		return fmt.Errorf("decode Docker response: %w", err)
	}
	return nil
}

func (c *Client) getBytes(ctx context.Context, endpoint string) ([]byte, string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+endpoint, nil)
	if err != nil {
		return nil, "", err
	}
	req.Header.Set("Accept", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, "", fmt.Errorf("connect to Docker at %s: %w", c.target, err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, "", fmt.Errorf("read Docker response: %w", err)
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		var apiErr struct {
			Message string `json:"message"`
		}
		_ = json.Unmarshal(body, &apiErr)
		message := strings.TrimSpace(apiErr.Message)
		if message == "" {
			message = strings.TrimSpace(string(body))
		}
		if message == "" {
			message = resp.Status
		}
		return nil, "", fmt.Errorf("Docker API %s: %s", resp.Status, message)
	}
	return body, resp.Header.Get("Content-Type"), nil
}

func decodeDockerStream(data []byte) (string, bool) {
	if len(data) < 8 {
		return "", false
	}
	reader := bytes.NewReader(data)
	var out bytes.Buffer
	frames := 0
	for reader.Len() > 0 {
		header := make([]byte, 8)
		if _, err := io.ReadFull(reader, header); err != nil {
			return "", false
		}
		if header[0] < 1 || header[0] > 3 || header[1] != 0 || header[2] != 0 || header[3] != 0 {
			return "", false
		}
		n := binary.BigEndian.Uint32(header[4:])
		if uint64(n) > uint64(reader.Len()) {
			return "", false
		}
		if _, err := io.CopyN(&out, reader, int64(n)); err != nil {
			return "", false
		}
		frames++
	}
	return out.String(), frames > 0
}
