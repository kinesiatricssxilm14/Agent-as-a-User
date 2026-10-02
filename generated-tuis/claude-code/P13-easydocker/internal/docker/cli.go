package docker

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"strings"
)

// CLIClient drives the real `docker` binary. It is the fallback used when no
// API socket answers, so toolm still shows real state on hosts where only the
// CLI is configured (for example when a credential helper or context is
// required to reach the daemon).
type CLIClient struct {
	bin string
}

// NewCLIClient locates the docker binary and verifies it can reach a daemon.
func NewCLIClient(ctx context.Context) (*CLIClient, error) {
	bin, err := exec.LookPath("docker")
	if err != nil {
		return nil, errors.New("docker binary not found in PATH")
	}
	c := &CLIClient{bin: bin}
	if _, err := c.run(ctx, "version", "--format", "{{.Server.Version}}"); err != nil {
		return nil, err
	}
	return c, nil
}

// Endpoint implements Client.
func (c *CLIClient) Endpoint() string { return "docker CLI (" + c.bin + ")" }

// run executes a docker subcommand and returns its stdout.
func (c *CLIClient) run(ctx context.Context, args ...string) ([]byte, error) {
	cmd := exec.CommandContext(ctx, c.bin, args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = err.Error()
		}
		if len(msg) > 300 {
			msg = msg[:300] + "..."
		}
		return nil, fmt.Errorf("docker %s: %s", strings.Join(args, " "), msg)
	}
	return stdout.Bytes(), nil
}

// runJSONLines runs a docker command that emits one JSON object per line and
// decodes each line into a fresh T.
func runJSONLines[T any](c *CLIClient, ctx context.Context, args ...string) ([]T, error) {
	out, err := c.run(ctx, args...)
	if err != nil {
		return nil, err
	}
	var list []T
	for _, line := range strings.Split(string(out), "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		var item T
		if err := json.Unmarshal([]byte(line), &item); err != nil {
			return nil, fmt.Errorf("decoding docker output: %w", err)
		}
		list = append(list, item)
	}
	return list, nil
}

// Version implements Client.
func (c *CLIClient) Version(ctx context.Context) (string, error) {
	out, err := c.run(ctx, "version", "--format", "{{.Server.Version}} (API {{.Server.APIVersion}})")
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

// Containers implements Client. `docker ps` output is normalised into the same
// shape the API returns so the UI code has a single representation.
func (c *CLIClient) Containers(ctx context.Context) ([]Container, error) {
	type psLine struct {
		ID      string `json:"ID"`
		Names   string `json:"Names"`
		Image   string `json:"Image"`
		Command string `json:"Command"`
		State   string `json:"State"`
		Status  string `json:"Status"`
		Ports   string `json:"Ports"`
	}
	lines, err := runJSONLines[psLine](c, ctx, "ps", "--all", "--no-trunc", "--format", "{{json .}}")
	if err != nil {
		return nil, err
	}
	out := make([]Container, 0, len(lines))
	for _, l := range lines {
		ct := Container{
			ID:      l.ID,
			Image:   l.Image,
			Command: strings.Trim(l.Command, `"`),
			State:   l.State,
			Status:  l.Status,
			Ports:   parsePortList(l.Ports),
		}
		for _, n := range strings.Split(l.Names, ",") {
			if n = strings.TrimSpace(n); n != "" {
				ct.Names = append(ct.Names, n)
			}
		}
		out = append(out, ct)
	}
	return out, nil
}

// parsePortList parses the comma separated port column of `docker ps`.
func parsePortList(s string) []Port {
	var ports []Port
	for _, entry := range strings.Split(s, ",") {
		entry = strings.TrimSpace(entry)
		if entry == "" {
			continue
		}
		var p Port
		var hostPart, containerPart string
		if i := strings.Index(entry, "->"); i >= 0 {
			hostPart, containerPart = entry[:i], entry[i+2:]
		} else {
			containerPart = entry
		}
		if i := strings.LastIndex(containerPart, "/"); i >= 0 {
			p.Type = containerPart[i+1:]
			containerPart = containerPart[:i]
		}
		fmt.Sscanf(containerPart, "%d", &p.PrivatePort)
		if hostPart != "" {
			if i := strings.LastIndex(hostPart, ":"); i >= 0 {
				p.IP = hostPart[:i]
				fmt.Sscanf(hostPart[i+1:], "%d", &p.PublicPort)
			} else {
				fmt.Sscanf(hostPart, "%d", &p.PublicPort)
			}
		}
		ports = append(ports, p)
	}
	return ports
}

// inspectOne runs `docker inspect` and decodes the single-element array it
// returns into out.
func (c *CLIClient) inspectOne(ctx context.Context, out any, args ...string) error {
	raw, err := c.run(ctx, append([]string{"inspect"}, args...)...)
	if err != nil {
		return err
	}
	trimmed := bytes.TrimSpace(raw)
	if len(trimmed) > 0 && trimmed[0] == '[' {
		var items []json.RawMessage
		if err := json.Unmarshal(trimmed, &items); err != nil {
			return err
		}
		if len(items) == 0 {
			return errors.New("no such object")
		}
		return json.Unmarshal(items[0], out)
	}
	return json.Unmarshal(trimmed, out)
}

// ContainerInspect implements Client.
func (c *CLIClient) ContainerInspect(ctx context.Context, id string) (*ContainerDetails, error) {
	var d ContainerDetails
	if err := c.inspectOne(ctx, &d, "--type", "container", id); err != nil {
		return nil, err
	}
	return &d, nil
}

// ContainerLogs implements Client.
func (c *CLIClient) ContainerLogs(ctx context.Context, id string) (string, error) {
	cmd := exec.CommandContext(ctx, c.bin, "logs", id)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	// docker logs writes the container's stderr stream to its own stderr, so
	// both buffers are part of the log output.
	combined := stdout.String() + stderr.String()
	if err != nil && strings.TrimSpace(combined) == "" {
		return "", fmt.Errorf("docker logs %s: %v", id, err)
	}
	return combined, nil
}

// Images implements Client.
func (c *CLIClient) Images(ctx context.Context) ([]Image, error) {
	type imgLine struct {
		ID         string `json:"ID"`
		Repository string `json:"Repository"`
		Tag        string `json:"Tag"`
	}
	lines, err := runJSONLines[imgLine](c, ctx, "images", "--no-trunc", "--format", "{{json .}}")
	if err != nil {
		return nil, err
	}
	out := make([]Image, 0, len(lines))
	for _, l := range lines {
		img := Image{ID: l.ID}
		if l.Repository != "" && l.Repository != "<none>" {
			img.RepoTags = []string{l.Repository + ":" + l.Tag}
		}
		// The human readable size column is lossy ("143MB"), so the exact byte
		// count comes from inspect instead.
		var d ImageDetails
		if err := c.inspectOne(ctx, &d, "--type", "image", l.ID); err == nil {
			img.Size = d.SizeBytes()
			if len(d.RepoTags) > 0 {
				img.RepoTags = d.RepoTags
			}
			img.RepoDigests = d.RepoDigests
		}
		out = append(out, img)
	}
	return out, nil
}

// ImageInspect implements Client.
func (c *CLIClient) ImageInspect(ctx context.Context, id string) (*ImageDetails, error) {
	var d ImageDetails
	if err := c.inspectOne(ctx, &d, "--type", "image", id); err != nil {
		return nil, err
	}
	return &d, nil
}

// Networks implements Client.
func (c *CLIClient) Networks(ctx context.Context) ([]Network, error) {
	type netLine struct {
		ID     string `json:"ID"`
		Name   string `json:"Name"`
		Driver string `json:"Driver"`
		Scope  string `json:"Scope"`
	}
	lines, err := runJSONLines[netLine](c, ctx, "network", "ls", "--no-trunc", "--format", "{{json .}}")
	if err != nil {
		return nil, err
	}
	out := make([]Network, 0, len(lines))
	for _, l := range lines {
		out = append(out, Network{ID: l.ID, Name: l.Name, Driver: l.Driver, Scope: l.Scope})
	}
	return out, nil
}

// NetworkInspect implements Client.
func (c *CLIClient) NetworkInspect(ctx context.Context, id string) (*Network, error) {
	var n Network
	if err := c.inspectOne(ctx, &n, "--type", "network", id); err != nil {
		return nil, err
	}
	return &n, nil
}

// Volumes implements Client.
func (c *CLIClient) Volumes(ctx context.Context) ([]Volume, error) {
	lines, err := runJSONLines[Volume](c, ctx, "volume", "ls", "--format", "{{json .}}")
	if err != nil {
		return nil, err
	}
	out := make([]Volume, 0, len(lines))
	for _, v := range lines {
		if v.Mountpoint == "" {
			if full, err := c.VolumeInspect(ctx, v.Name); err == nil {
				v = *full
			}
		}
		out = append(out, v)
	}
	return out, nil
}

// VolumeInspect implements Client.
func (c *CLIClient) VolumeInspect(ctx context.Context, name string) (*Volume, error) {
	var v Volume
	if err := c.inspectOne(ctx, &v, "--type", "volume", name); err != nil {
		return nil, err
	}
	return &v, nil
}
