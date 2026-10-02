package ui

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"

	"toolm/internal/docker"
)

const labelWidth = 16

func shortID(id string) string {
	id = strings.TrimPrefix(id, "sha256:")
	if len(id) > 12 {
		return id[:12]
	}
	return id
}

func formatSizeMB(bytes int64) string {
	return fmt.Sprintf("%.1f MB", float64(bytes)/(1024*1024))
}

func formatTime(s string) string {
	if s == "" || s == "0001-01-01T00:00:00Z" {
		return "-"
	}
	t, err := time.Parse(time.RFC3339Nano, s)
	if err != nil {
		return s
	}
	return t.Local().Format("2006-01-02 15:04:05 MST")
}

func firstRepoTag(tags []string) (string, string) {
	if len(tags) == 0 || tags[0] == "" || tags[0] == "<none>:<none>" {
		return "<none>", "<none>"
	}
	t := tags[0]
	if i := strings.LastIndex(t, ":"); i >= 0 {
		return t[:i], t[i+1:]
	}
	return t, "latest"
}

func imageDisplayName(img docker.Image) string {
	repo, tag := firstRepoTag(img.RepoTags)
	return repo + ":" + tag
}

func shellJoin(args []string) string {
	var b strings.Builder
	for i, a := range args {
		if i > 0 {
			b.WriteString(" ")
		}
		if strings.ContainsAny(a, " \t\n\"'") {
			b.WriteString("'" + strings.ReplaceAll(a, "'", `'\''`) + "'")
		} else {
			b.WriteString(a)
		}
	}
	return b.String()
}

func formatContainerPorts(ports []docker.Port) string {
	var parts []string
	for _, p := range ports {
		if p.PublicPort > 0 {
			if p.IP != "" && p.IP != "::" && p.IP != "0.0.0.0" {
				parts = append(parts, fmt.Sprintf("%s:%d->%d/%s", p.IP, p.PublicPort, p.PrivatePort, p.Type))
			} else {
				parts = append(parts, fmt.Sprintf("%d->%d/%s", p.PublicPort, p.PrivatePort, p.Type))
			}
		} else {
			parts = append(parts, fmt.Sprintf("%d/%s", p.PrivatePort, p.Type))
		}
	}
	return strings.Join(parts, ", ")
}

// kv renders a detail row with a padded label.
func kv(label, value string) string {
	if value == "" {
		value = "-"
	}
	return detailLabelStyle.Render(pad(label, labelWidth)) + " " + detailValueStyle.Render(value)
}

// addSection appends a labelled section, indenting continuation lines.
func addSection(lines *[]string, label string, values []string) {
	indent := detailValueStyle.Render(strings.Repeat(" ", labelWidth+1))
	if len(values) == 0 {
		*lines = append(*lines, kv(label, ""))
		return
	}
	*lines = append(*lines, kv(label, values[0]))
	for _, v := range values[1:] {
		*lines = append(*lines, indent+detailValueStyle.Render(v))
	}
}

func commandString(insp docker.ContainerInspect) string {
	var parts []string
	if insp.Config != nil {
		parts = append(parts, insp.Config.Entrypoint...)
		parts = append(parts, insp.Config.Cmd...)
	}
	if len(parts) > 0 {
		return shellJoin(parts)
	}
	if insp.Path != "" {
		all := append([]string{insp.Path}, insp.Args...)
		return shellJoin(all)
	}
	return ""
}

func imageOf(insp docker.ContainerInspect) string {
	if insp.Config != nil && insp.Config.Image != "" {
		return insp.Config.Image
	}
	return insp.Image
}

func portLines(insp docker.ContainerInspect) []string {
	var out []string
	bindings := map[string][]docker.PortBinding{}
	if insp.HostConfig != nil && insp.HostConfig.PortBindings != nil {
		bindings = insp.HostConfig.PortBindings
	} else if insp.NetworkSettings != nil && insp.NetworkSettings.Ports != nil {
		bindings = insp.NetworkSettings.Ports
	}
	keys := make([]string, 0, len(bindings))
	for k := range bindings {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, exposed := range keys {
		for _, b := range bindings[exposed] {
			if b.HostPort == "" {
				out = append(out, fmt.Sprintf("%s (unpublished)", exposed))
				continue
			}
			host := b.HostIP
			if host == "" {
				host = "0.0.0.0"
			}
			out = append(out, fmt.Sprintf("%s:%s -> %s", host, b.HostPort, exposed))
		}
	}
	return out
}

func mountLines(insp docker.ContainerInspect) []string {
	var out []string
	for _, m := range insp.Mounts {
		src := m.Source
		if m.Type == "volume" && m.Name != "" {
			src = m.Name
		}
		if src == "" {
			src = "-"
		}
		line := fmt.Sprintf("%s: %s -> %s", m.Type, src, m.Destination)
		if !m.RW {
			line += " (ro)"
		}
		out = append(out, line)
	}
	return out
}

func networkLines(ns docker.NetworkSettings) []string {
	var out []string
	names := make([]string, 0, len(ns.Networks))
	for n := range ns.Networks {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, n := range names {
		ep := ns.Networks[n]
		out = append(out, fmt.Sprintf("%s: %s", n, ep.IPAddress))
	}
	return out
}

func exposedPorts(m map[string]struct{}) []string {
	if len(m) == 0 {
		return nil
	}
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func mapLines(m map[string]string) []string {
	if len(m) == 0 {
		return nil
	}
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	out := make([]string, 0, len(m))
	for _, k := range keys {
		out = append(out, fmt.Sprintf("%s=%s", k, m[k]))
	}
	return out
}

func strBool(b bool) string {
	return strconv.FormatBool(b)
}

func truncate(s string, w int) string {
	if w <= 0 {
		return ""
	}
	r := []rune(s)
	if len(r) <= w {
		return s
	}
	if w == 1 {
		return "…"
	}
	return string(r[:w-1]) + "…"
}

func pad(s string, w int) string {
	r := []rune(s)
	if len(r) >= w {
		return truncate(s, w)
	}
	return s + strings.Repeat(" ", w-len(r))
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}
