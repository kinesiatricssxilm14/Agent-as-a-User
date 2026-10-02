package tui

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/charmbracelet/lipgloss"

	"toolm/dockerapi"
)

var (
	colorPrimary = lipgloss.Color("39")
	colorAccent  = lipgloss.Color("212")
	colorMuted   = lipgloss.Color("241")
	colorGood    = lipgloss.Color("42")
	colorBad     = lipgloss.Color("196")

	titleStyle     = lipgloss.NewStyle().Bold(true).Foreground(colorPrimary)
	activeTabStyle = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("0")).Background(colorPrimary).Padding(0, 1)
	tabStyle       = lipgloss.NewStyle().Foreground(lipgloss.Color("250")).Padding(0, 1)
	headerStyle    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("255")).Background(lipgloss.Color("236"))
	selectedStyle  = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("0")).Background(colorAccent)
	mutedStyle     = lipgloss.NewStyle().Foreground(colorMuted)
	keyStyle       = lipgloss.NewStyle().Bold(true).Foreground(colorPrimary)
	errorStyle     = lipgloss.NewStyle().Foreground(colorBad)
	goodStyle      = lipgloss.NewStyle().Foreground(colorGood)
	labelStyle     = lipgloss.NewStyle().Bold(true).Foreground(colorPrimary)
)

func (m Model) View() string {
	if m.width <= 0 || m.height <= 0 {
		return "Starting toolm…"
	}
	var b strings.Builder
	b.WriteString(m.renderTop())
	b.WriteByte('\n')

	switch m.screen {
	case listScreen:
		b.WriteString(m.renderList())
	case detailScreen:
		b.WriteString(m.renderTextView("Details · "+m.detailTitle, m.detailLines()))
	case logsScreen:
		b.WriteString(m.renderTextView("Logs · "+m.logsTitle, m.logDisplayLines()))
	case helpScreen:
		b.WriteString(m.renderTextView("Keyboard help", helpLines()))
	}
	b.WriteByte('\n')
	b.WriteString(m.renderFooter())
	return fitHeight(b.String(), m.height)
}

func (m Model) renderTop() string {
	left := titleStyle.Render("toolm") + mutedStyle.Render("  Docker resource manager")
	target := mutedStyle.Render("API: " + m.client.Target())
	gap := max(1, m.width-lipgloss.Width(left)-lipgloss.Width(target))
	line := left + strings.Repeat(" ", gap) + target
	if lipgloss.Width(line) > m.width {
		line = truncateANSI(left, m.width)
	}

	tabs := []string{"1 Containers", "2 Images", "3 Networks", "4 Volumes"}
	var rendered []string
	for i, tab := range tabs {
		if resource(i) == m.active {
			rendered = append(rendered, activeTabStyle.Render(tab))
		} else {
			rendered = append(rendered, tabStyle.Render(tab))
		}
	}
	return line + "\n" + truncateANSI(strings.Join(rendered, " "), m.width)
}

func (m Model) renderList() string {
	if m.loading[m.active] && m.itemCount(m.active) == 0 {
		return padLines("\n  Loading "+resourceName(m.active)+" from Docker API…", m.listHeight()+1)
	}
	if err := m.lastErr[m.active]; err != nil && m.itemCount(m.active) == 0 {
		return padLines(errorStyle.Render("Docker API error: "+err.Error())+"\n\n"+mutedStyle.Render("Press r to retry. Verify the Docker socket or DOCKER_HOST."), m.listHeight()+1)
	}

	var header string
	var rows []string
	switch m.active {
	case containers:
		header, rows = m.containerRows()
	case images:
		header, rows = m.imageRows()
	case networks:
		header, rows = m.networkRows()
	case volumes:
		header, rows = m.volumeRows()
	}
	var b strings.Builder
	b.WriteString(headerStyle.Width(m.width).Render(truncatePlain(header, m.width)))
	b.WriteByte('\n')
	if len(rows) == 0 {
		message := "No " + resourceName(m.active) + " found."
		if m.filter != "" {
			message = "No matches for \"" + m.filter + "\"."
		}
		b.WriteString(mutedStyle.Render("  " + message))
		return padLines(b.String(), m.listHeight()+1)
	}
	start := clamp(m.offset[m.active], 0, max(0, len(rows)-1))
	end := min(len(rows), start+m.listHeight())
	for i := start; i < end; i++ {
		row := truncatePlain(rows[i], m.width)
		if i == m.cursor[m.active] {
			b.WriteString(selectedStyle.Width(m.width).Render(row))
		} else {
			b.WriteString(row)
		}
		if i != end-1 {
			b.WriteByte('\n')
		}
	}
	return padLines(b.String(), m.listHeight()+1)
}

func (m Model) containerRows() (string, []string) {
	items := m.filteredContainers()
	widths := tableWidths(m.width, []int{24, 30, 12, 60}, []int{8, 10, 7, 12})
	nameW, imageW, stateW := widths[0], widths[1], widths[2]
	header := fmt.Sprintf("%-*s  %-*s  %-*s  %s", nameW, "NAME", imageW, "IMAGE", stateW, "STATE", "STATUS")
	rows := make([]string, 0, len(items))
	for _, v := range items {
		rows = append(rows, fmt.Sprintf("%-*s  %-*s  %-*s  %s", nameW, truncatePlain(containerName(v), nameW), imageW, truncatePlain(v.Image, imageW), stateW, truncatePlain(v.State, stateW), v.Status))
	}
	return header, rows
}

func (m Model) imageRows() (string, []string) {
	items := m.filteredImages()
	widths := tableWidths(m.width, []int{32, 20, 12, 40}, []int{10, 7, 8, 12})
	nameW, tagW, sizeW := widths[0], widths[1], widths[2]
	header := fmt.Sprintf("%-*s  %-*s  %*s  %s", nameW, "NAME", tagW, "TAG", sizeW, "SIZE", "IMAGE ID")
	rows := make([]string, 0, len(items))
	for _, v := range items {
		name, tag := imageNameTag(v)
		rows = append(rows, fmt.Sprintf("%-*s  %-*s  %*s  %s", nameW, truncatePlain(name, nameW), tagW, truncatePlain(tag, tagW), sizeW, imageSize(v.Size), shortID(v.ID)))
	}
	return header, rows
}

func (m Model) networkRows() (string, []string) {
	items := m.filteredNetworks()
	widths := tableWidths(m.width, []int{36, 18, 50}, []int{10, 8, 12})
	nameW, driverW := widths[0], widths[1]
	header := fmt.Sprintf("%-*s  %-*s  %s", nameW, "NAME", driverW, "DRIVER", "NETWORK ID")
	rows := make([]string, 0, len(items))
	for _, v := range items {
		rows = append(rows, fmt.Sprintf("%-*s  %-*s  %s", nameW, truncatePlain(v.Name, nameW), driverW, truncatePlain(v.Driver, driverW), shortID(v.ID)))
	}
	return header, rows
}

func (m Model) volumeRows() (string, []string) {
	items := m.filteredVolumes()
	widths := tableWidths(m.width, []int{45, 18, 60}, []int{10, 8, 12})
	nameW, driverW := widths[0], widths[1]
	header := fmt.Sprintf("%-*s  %-*s  %s", nameW, "NAME", driverW, "DRIVER", "MOUNTPOINT")
	rows := make([]string, 0, len(items))
	for _, v := range items {
		rows = append(rows, fmt.Sprintf("%-*s  %-*s  %s", nameW, truncatePlain(v.Name, nameW), driverW, truncatePlain(v.Driver, driverW), v.Mountpoint))
	}
	return header, rows
}

func (m Model) renderTextView(title string, lines []string) string {
	wrapped := wrapAll(lines, max(10, m.width-2))
	h := m.contentHeight()
	offset := clamp(m.textOffset, 0, max(0, len(wrapped)-h))
	end := min(len(wrapped), offset+h)
	var b strings.Builder
	b.WriteString(titleStyle.Render(truncatePlain(title, m.width)))
	b.WriteByte('\n')
	for i := offset; i < end; i++ {
		b.WriteString(truncatePlain(wrapped[i], m.width))
		if i != end-1 {
			b.WriteByte('\n')
		}
	}
	return padLines(b.String(), h+1)
}

func (m Model) renderFooter() string {
	var help string
	switch m.screen {
	case listScreen:
		help = keys("↑/↓", "move", "Tab", "view", "Enter", "details", "/", "filter", "r", "refresh")
		if m.active == containers {
			help += "  " + keys("l", "logs")
		}
		help += "  " + keys("?", "help", "q", "quit")
	case detailScreen, logsScreen:
		help = keys("↑/↓ PgUp/PgDn", "scroll", "g/G", "top/end", "Esc", "back", "?", "help")
	case helpScreen:
		help = keys("↑/↓ PgUp/PgDn", "scroll", "Esc/?", "close")
	}
	status := m.status
	if m.filtering {
		status = "Filter: " + m.filter + "█  (Enter apply · Esc clear)"
	} else if m.filter != "" && m.screen == listScreen {
		status = fmt.Sprintf("Filter: %q · %d matches", m.filter, m.itemCount(m.active))
	}
	if m.loading[m.active] {
		status = "Working… " + status
	}
	if status == "" {
		status = fmt.Sprintf("%d %s", m.itemCount(m.active), resourceName(m.active))
	}
	return truncateANSI(help, m.width) + "\n" + truncatePlain(status, m.width)
}

func (m Model) detailLines() []string {
	switch m.active {
	case containers:
		if m.containerDetail == nil {
			return []string{"No container detail loaded."}
		}
		v := m.containerDetail
		command := strings.TrimSpace(strings.Join(append([]string{v.Path}, v.Args...), " "))
		if command == "" {
			command = strings.Join(v.Config.Cmd, " ")
		}
		lines := []string{
			field("Name", cleanName(v.Name)), field("Image", valueOr(v.Config.Image, "<none>")), field("ID", v.ID),
			field("Created", formatDockerTime(v.Created)), field("Status", v.State.Status), field("Running", strconv.FormatBool(v.State.Running)),
			field("Start command", valueOr(command, "<none>")), field("Hostname", valueOr(v.Config.Hostname, "<none>")),
			field("Working directory", valueOr(v.Config.WorkingDir, "<none>")), field("Started", formatDockerTime(v.State.StartedAt)),
			field("Finished", formatDockerTime(v.State.FinishedAt)), field("Exit code", strconv.Itoa(v.State.ExitCode)), "", labelStyle.Render("Port mappings"),
		}
		ports := formatInspectPorts(v.NetworkSettings.Ports)
		if len(ports) == 0 {
			lines = append(lines, "  <none>")
		} else {
			for _, p := range ports {
				lines = append(lines, "  "+p)
			}
		}
		lines = append(lines, "", labelStyle.Render("Networks"))
		if len(v.NetworkSettings.Networks) == 0 {
			lines = append(lines, "  <none>")
		} else {
			names := sortedKeys(v.NetworkSettings.Networks)
			for _, name := range names {
				n := v.NetworkSettings.Networks[name]
				lines = append(lines, fmt.Sprintf("  %s  IP %s  Gateway %s", name, valueOr(n.IPAddress, "-"), valueOr(n.Gateway, "-")))
			}
		}
		lines = append(lines, "", labelStyle.Render("Mounts"))
		if len(v.Mounts) == 0 {
			lines = append(lines, "  <none>")
		} else {
			for _, mount := range v.Mounts {
				lines = append(lines, fmt.Sprintf("  %s  %s → %s  rw=%t", mount.Type, mount.Source, mount.Destination, mount.RW))
			}
		}
		return lines
	case images:
		if m.imageDetail == nil {
			return []string{"No image detail loaded."}
		}
		v := *m.imageDetail
		name, tag := imageNameTag(v)
		return []string{field("Name", name), field("Tag", tag), field("Size", imageSize(v.Size)), field("Image ID", v.ID), field("Created", formatUnix(v.Created)), field("Repository tags", valueOr(strings.Join(v.RepoTags, ", "), "<none>"))}
	case networks:
		if m.networkDetail == nil {
			return []string{"No network detail loaded."}
		}
		v := *m.networkDetail
		return []string{field("Name", v.Name), field("Driver", v.Driver), field("Network ID", v.ID), field("Scope", v.Scope), field("Internal", strconv.FormatBool(v.Internal)), field("Created", formatDockerTime(v.Created))}
	case volumes:
		if m.volumeDetail == nil {
			return []string{"No volume detail loaded."}
		}
		v := *m.volumeDetail
		lines := []string{field("Name", v.Name), field("Driver", v.Driver), field("Mountpoint", v.Mountpoint), field("Scope", v.Scope), field("Created", formatDockerTime(v.CreatedAt))}
		if len(v.Labels) > 0 {
			lines = append(lines, "", labelStyle.Render("Labels"))
			for _, k := range sortedKeys(v.Labels) {
				lines = append(lines, "  "+k+" = "+v.Labels[k])
			}
		}
		return lines
	}
	return nil
}

func (m Model) logDisplayLines() []string {
	if len(m.textLines) == 0 {
		return []string{"<no log output>"}
	}
	return m.textLines
}

func helpLines() []string {
	return []string{
		labelStyle.Render("Global"),
		"  1 / 2 / 3 / 4       Containers / Images / Networks / Volumes",
		"  Tab / Shift+Tab     Next / previous resource view",
		"  ?                   Open or close this help",
		"  Ctrl+C              Quit from anywhere",
		"",
		labelStyle.Render("Resource lists"),
		"  ↑ / ↓ or k / j      Move selection",
		"  PgUp / PgDn         Move one screen",
		"  g / G               First / last item",
		"  Enter or d          Open selected item details",
		"  l                   Open complete container logs (Containers only)",
		"  /                   Filter the current list; Enter applies, Esc clears",
		"  r                   Refresh current resource from Docker",
		"  q                   Quit",
		"",
		labelStyle.Render("Details, logs, and help"),
		"  ↑ / ↓ or k / j      Scroll one line",
		"  PgUp / PgDn         Scroll one screen",
		"  g / G               Jump to top / end",
		"  Esc / Backspace / q Return to the resource list",
		"",
		"toolm reads authentic state through the Docker Engine HTTP API. Set",
		"DOCKER_HOST to unix://path, tcp://host:port, http://…, or https://…",
		"when the daemon/mock is not at /var/run/docker.sock.",
	}
}

func keys(parts ...string) string {
	var out []string
	for i := 0; i+1 < len(parts); i += 2 {
		out = append(out, keyStyle.Render(parts[i])+" "+mutedStyle.Render(parts[i+1]))
	}
	return strings.Join(out, "  ")
}

func field(label, value string) string {
	return fmt.Sprintf("%-18s %s", labelStyle.Render(label+":"), valueOr(value, "<none>"))
}
func resourceName(r resource) string {
	return [...]string{"containers", "images", "networks", "volumes"}[r]
}
func containerName(v dockerapi.Container) string {
	if len(v.Names) == 0 {
		return shortID(v.ID)
	}
	return cleanName(v.Names[0])
}
func cleanName(v string) string { return strings.TrimPrefix(v, "/") }
func shortID(v string) string {
	v = strings.TrimPrefix(v, "sha256:")
	if len(v) > 12 {
		return v[:12]
	}
	return v
}
func imageRepository(v dockerapi.Image) string { n, t := imageNameTag(v); return n + ":" + t }
func imageNameTag(v dockerapi.Image) (string, string) {
	if len(v.RepoTags) == 0 || v.RepoTags[0] == "<none>:<none>" {
		return "<none>", "<none>"
	}
	tag := v.RepoTags[0]
	i := strings.LastIndex(tag, ":")
	if i > strings.LastIndex(tag, "/") {
		return tag[:i], tag[i+1:]
	}
	return tag, "<none>"
}
func imageSize(size int64) string { return fmt.Sprintf("%.1f MB", float64(size)/1_000_000) }
func formatUnix(value int64) string {
	if value <= 0 {
		return "<unknown>"
	}
	return time.Unix(value, 0).Local().Format("2006-01-02 15:04:05 MST")
}
func formatDockerTime(value string) string {
	if value == "" || strings.HasPrefix(value, "0001-01-01") {
		return "<none>"
	}
	if t, err := time.Parse(time.RFC3339Nano, value); err == nil {
		return t.Local().Format("2006-01-02 15:04:05 MST")
	}
	return value
}
func formatInspectPorts(ports map[string][]dockerapi.PortBinding) []string {
	if len(ports) == 0 {
		return nil
	}
	keys := sortedKeys(ports)
	var out []string
	for _, containerPort := range keys {
		bindings := ports[containerPort]
		if len(bindings) == 0 {
			out = append(out, containerPort+" (not published)")
			continue
		}
		for _, b := range bindings {
			out = append(out, fmt.Sprintf("%s:%s → %s", valueOr(b.HostIP, "0.0.0.0"), b.HostPort, containerPort))
		}
	}
	return out
}
func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
func splitLines(text string) []string {
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.TrimSuffix(text, "\n")
	if text == "" {
		return nil
	}
	return strings.Split(text, "\n")
}
func valueOr(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func tableWidths(total int, preferred, minimum []int) []int {
	count := len(preferred)
	gaps := (count - 1) * 2
	available := max(count, total-gaps)
	widths := append([]int(nil), preferred...)
	sum := 0
	for _, w := range widths {
		sum += w
	}
	for sum > available {
		changed := false
		for i := range widths {
			if sum <= available {
				break
			}
			if widths[i] > minimum[i] {
				widths[i]--
				sum--
				changed = true
			}
		}
		if !changed {
			break
		}
	}
	return widths
}
func wrapAll(lines []string, width int) []string {
	var out []string
	for _, line := range lines {
		out = append(out, wrapLine(line, width)...)
	}
	if len(out) == 0 {
		return []string{""}
	}
	return out
}
func wrapLine(line string, width int) []string {
	if width <= 0 || utf8.RuneCountInString(line) <= width {
		return []string{line}
	}
	indent := ""
	for _, r := range line {
		if r == ' ' {
			indent += " "
		} else {
			break
		}
	}
	continuation := indent
	var out []string
	rest := []rune(line)
	for len(rest) > width {
		cut := width
		for i := width; i > len([]rune(indent))+1; i-- {
			if rest[i-1] == ' ' {
				cut = i
				break
			}
		}
		out = append(out, string(rest[:cut]))
		rest = []rune(continuation + strings.TrimLeft(string(rest[cut:]), " "))
	}
	out = append(out, string(rest))
	return out
}
func truncatePlain(s string, width int) string {
	if width <= 0 {
		return ""
	}
	r := []rune(s)
	if len(r) <= width {
		return s
	}
	if width == 1 {
		return "…"
	}
	return string(r[:width-1]) + "…"
}
func truncateANSI(s string, width int) string {
	if lipgloss.Width(s) <= width {
		return s
	}
	return lipgloss.NewStyle().MaxWidth(width).Render(s)
}
func padLines(s string, height int) string {
	lines := strings.Count(s, "\n") + 1
	if lines >= height {
		return s
	}
	return s + strings.Repeat("\n", height-lines)
}
func fitHeight(s string, height int) string {
	lines := strings.Split(s, "\n")
	if len(lines) > height {
		lines = lines[:height]
	}
	return strings.Join(lines, "\n")
}
func clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}
func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
