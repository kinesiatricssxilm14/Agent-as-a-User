package docker

import (
	"bufio"
	"context"
	"encoding/binary"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
)

// ListContainers returns all containers (running and stopped).
func (c *Client) ListContainers(ctx context.Context) ([]Container, error) {
	q := url.Values{}
	q.Set("all", "1")
	var out []Container
	if err := c.do(ctx, http.MethodGet, "/containers/json", q, nil, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// InspectContainer returns detailed container information.
func (c *Client) InspectContainer(ctx context.Context, id string) (*ContainerInspect, error) {
	var out ContainerInspect
	if err := c.do(ctx, http.MethodGet, "/containers/"+id+"/json", nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// ContainerLogs fetches the complete log output for a container.
func (c *Client) ContainerLogs(ctx context.Context, id string) (string, error) {
	tty := false
	if insp, err := c.InspectContainer(ctx, id); err == nil && insp.Config != nil {
		tty = insp.Config.Tty
	}

	q := url.Values{}
	q.Set("stdout", "1")
	q.Set("stderr", "1")
	q.Set("tail", "all")

	u := c.baseURL + "/containers/" + id + "/logs?" + q.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return "", err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		data, _ := io.ReadAll(io.LimitReader(resp.Body, 8192))
		return "", fmt.Errorf("logs %s: %s (%s)", id, resp.Status, strings.TrimSpace(string(data)))
	}
	if tty {
		data, err := io.ReadAll(resp.Body)
		return string(data), err
	}
	return demuxLogs(resp.Body)
}

// demuxLogs decodes the Docker multiplexed stream (stdout/stderr frames) and
// falls back to a raw read when the stream is not multiplexed.
func demuxLogs(r io.Reader) (string, error) {
	br := bufio.NewReader(r)
	var sb strings.Builder
	hdr := make([]byte, 8)
	for {
		n, err := io.ReadFull(br, hdr)
		if err == io.EOF && n == 0 {
			break
		}
		if err != nil && err != io.ErrUnexpectedEOF {
			break
		}
		if n > 0 && (hdr[1] != 0 || hdr[2] != 0 || hdr[3] != 0 || hdr[0] > 2) {
			// Not a valid multiplex header: treat the stream as raw text.
			sb.Write(hdr[:n])
			rest, _ := io.ReadAll(br)
			sb.Write(rest)
			break
		}
		if n < 8 {
			break
		}
		size := int(binary.BigEndian.Uint32(hdr[4:8]))
		buf := make([]byte, size)
		if _, err := io.ReadFull(br, buf); err != nil {
			sb.Write(buf)
			break
		}
		sb.Write(buf)
	}
	return sb.String(), nil
}

// ContainerStart starts a stopped container.
func (c *Client) ContainerStart(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodPost, "/containers/"+id+"/start", nil, nil, nil)
}

// ContainerStop stops a running container.
func (c *Client) ContainerStop(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodPost, "/containers/"+id+"/stop", nil, nil, nil)
}

// ContainerRestart restarts a container.
func (c *Client) ContainerRestart(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodPost, "/containers/"+id+"/restart", nil, nil, nil)
}

// ContainerRemove removes a container.
func (c *Client) ContainerRemove(ctx context.Context, id string, force bool) error {
	q := url.Values{}
	if force {
		q.Set("force", "1")
	}
	return c.do(ctx, http.MethodDelete, "/containers/"+id, q, nil, nil)
}
