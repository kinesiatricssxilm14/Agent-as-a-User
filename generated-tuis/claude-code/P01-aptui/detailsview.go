package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// The details pane. The spec is explicit here: after selecting a package the
// pane must show the full description and *every* dependency package name,
// listed one by one, in a fixed area of the same screen. So this renders one
// dependency per line — never a comma-joined summary — and the pane scrolls
// rather than paginating, keeping all of it reachable without leaving the view.

// renderDetails builds the pane's full content. The result is handed to a
// viewport, so it may be taller than the pane; the user scrolls it with Tab then
// ↑/↓, and a scroll indicator in the pane title says how much is off-screen.
func (m *model) renderDetails(width int) string {
	if width <= 0 {
		return ""
	}

	p := m.selected()
	if p == nil {
		return m.styles.ListEmpty.Render("no package selected")
	}

	var lines []string
	add := func(s string) { lines = append(lines, s) }

	// The package name is not repeated here: the pane title carries it and stays
	// put while this content scrolls, so it remains visible either way. The state
	// line leads instead, because it is what the pending decision turns on.
	lines = append(lines, m.renderVersionLines(p, width)...)

	if m.detailsLoading {
		add("")
		add(m.styles.ListEmpty.Render(m.spin.View() + " reading package information…"))
		return strings.Join(lines, "\n")
	}
	if m.detailsErr != nil {
		add("")
		add(m.styles.StatusError.Render("cannot read package information:"))
		lines = append(lines, m.wrapStyled(m.detailsErr.Error(), width, m.styles.DetailText)...)
		return strings.Join(lines, "\n")
	}

	det := m.detailsPkg
	if det == nil {
		add("")
		add(m.styles.ListEmpty.Render("no information available"))
		return strings.Join(lines, "\n")
	}

	// Metadata fields, one per line so every value is visible at once.
	lines = append(lines, m.renderFields(p, det, width)...)

	// Description: synopsis then the full long text, wrapped to the pane.
	if det.Synopsis != "" || det.Long != "" {
		add("")
		add(m.styles.DetailSection.Render("Description"))
		if det.Synopsis != "" {
			lines = append(lines, m.wrapStyled(det.Synopsis, width, m.styles.DetailValue)...)
		}
		if det.Long != "" {
			if det.Synopsis != "" {
				add("")
			}
			lines = append(lines, m.wrapStyled(det.Long, width, m.styles.DetailText)...)
		}
	}

	// A virtual package has providers instead of dependencies; both are listed
	// one name per line.
	if det.Virtual {
		add("")
		add(m.styles.DetailSection.Render(fmt.Sprintf("Provided by (%d)", len(det.Providers))))
		for _, name := range det.Providers {
			add(m.styles.DetailDep.Render(truncate("  "+name, width)))
		}
	}

	// Relationships. Every group apt reported is shown, and every entry within
	// it gets its own line.
	if len(det.Relations) > 0 {
		for _, group := range det.Relations {
			add("")
			add(m.styles.DetailSection.Render(
				fmt.Sprintf("%s (%d)", group.Field, len(group.Deps))))
			lines = append(lines, m.renderDependencyLines(group, width)...)
		}
	} else if !det.Virtual {
		add("")
		add(m.styles.DetailSection.Render("Dependencies (0)"))
		add(m.styles.ListEmpty.Render("  this package depends on nothing"))
	}

	if det.Origin != "" {
		add("")
		lines = append(lines, m.wrapStyled("source: "+det.Origin, width, m.styles.DetailField)...)
	}

	return strings.Join(lines, "\n")
}

// renderVersionLines shows the installed and candidate versions, plus the
// upgrade target when there is one.
func (m *model) renderVersionLines(p *pkg, width int) []string {
	var lines []string

	state := "not installed"
	style := m.styles.DetailField
	switch {
	case p.Installed && p.Upgradable:
		state = fmt.Sprintf("installed %s → upgradable to %s", p.InstalledVersion, p.UpgradeTarget)
		style = m.styles.MarkUpgradable
	case p.Installed:
		state = "installed " + p.InstalledVersion + " (up to date)"
		style = m.styles.MarkInstalled
	case p.ResidualConfig():
		state = "removed, configuration files remain (" + p.Status + ")"
		style = m.styles.MarkResidual
	case p.Available:
		state = "not installed, available " + p.AvailableVersion
	}

	lines = append(lines, style.Render(truncate(state, width)))
	return lines
}

// detailField is one metadata row.
type detailField struct {
	label string
	value string
}

// renderFields lays out the metadata block. Fields with no value are skipped so
// the pane does not fill with empty labels.
func (m *model) renderFields(p *pkg, det *pkgDetails, width int) []string {
	fields := []detailField{
		{"Section", firstNonEmpty(det.Section, p.Section)},
		{"Priority", det.Priority},
		{"Architecture", det.Architecture},
		{"Installed-Size", formatSizeKB(det.InstalledSize)},
		{"Download-Size", formatSizeBytes(det.Size)},
		{"Source", det.Source},
		{"Homepage", det.Homepage},
		{"Maintainer", det.Maintainer},
	}

	// Align the values into a column so the block scans vertically.
	labelWidth := 0
	for _, f := range fields {
		if f.value != "" && len(f.label) > labelWidth {
			labelWidth = len(f.label)
		}
	}
	if labelWidth == 0 {
		return nil
	}

	var lines []string
	for _, f := range fields {
		if f.value == "" {
			continue
		}

		label := m.styles.DetailField.Render(pad(f.label, labelWidth) + "  ")
		valueWidth := width - labelWidth - 2

		// A long value (a maintainer address, a URL) wraps under its label
		// rather than being cut off, so nothing is hidden.
		wrapped := wrapText(f.value, max(1, valueWidth))
		for i, line := range wrapped {
			if i == 0 {
				lines = append(lines, label+m.styles.DetailValue.Render(line))
				continue
			}
			lines = append(lines,
				strings.Repeat(" ", labelWidth+2)+m.styles.DetailValue.Render(line))
		}
	}
	return lines
}

// renderDependencyLines renders one dependency per line: the package name, its
// version constraint, and a marker showing whether it is installed. Alternatives
// ("a | b") are listed under the first name so the choice is visible without
// collapsing the entry onto one crowded line.
func (m *model) renderDependencyLines(group relationGroup, width int) []string {
	var lines []string

	for _, dep := range group.Deps {
		lines = append(lines, m.renderOneDependency(dep, "  ", width))

		// Each alternative gets its own line too, indented under the primary.
		for _, alt := range dep.Alternatives {
			lines = append(lines, m.renderOneDependency(alt, "    | ", width))
		}
	}
	return lines
}

// renderOneDependency renders a single dependency atom with its install marker.
func (m *model) renderOneDependency(dep dependency, indent string, width int) string {
	mark, markStyle := m.dependencyMark(dep.Name)

	name := dep.Name
	if dep.Arch != "" {
		name += ":" + dep.Arch
	}

	// Name first, then the constraint, so a column of names is easy to scan.
	text := indent + name
	line := markStyle.Render(mark) + " " + m.styles.DetailDep.Render(text)

	if dep.Version != "" {
		constraint := " (" + dep.Version + ")"
		// Only append the constraint if it fits; the name must never be cut off
		// to make room for it.
		if lipgloss.Width(line)+lipgloss.Width(constraint) <= width {
			line += m.styles.DetailDepAlt.Render(constraint)
		}
	}
	return truncate(line, width)
}

// dependencyMark reports whether a dependency is satisfied on this system, using
// the same store the list renders from. This is what makes the pane actionable:
// an administrator can see at a glance which dependencies are missing.
func (m *model) dependencyMark(name string) (string, lipgloss.Style) {
	p := m.store.lookup(name)
	switch {
	case p != nil && p.Installed:
		return "●", m.styles.MarkInstalled
	case p != nil && p.Available:
		return "○", m.styles.MarkAvailable
	default:
		// Not in the dpkg database and not in the index: usually a virtual
		// package name satisfied by some provider.
		return "·", m.styles.MarkAvailable
	}
}

// wrapStyled wraps text to the pane width and applies a style per line, so that
// styling does not interfere with the width calculation.
func (m *model) wrapStyled(text string, width int, style lipgloss.Style) []string {
	var out []string
	for _, line := range wrapText(text, width) {
		if line == "" {
			out = append(out, "")
			continue
		}
		out = append(out, style.Render(line))
	}
	return out
}

// detailsScrollInfo reports how much of the pane is off-screen, so the user knows
// to scroll rather than assuming they are seeing everything.
func (m *model) detailsScrollInfo() string {
	if m.details.Height <= 0 {
		return ""
	}
	total := m.details.TotalLineCount()
	if total <= m.details.Height {
		return ""
	}
	return fmt.Sprintf("%d%%", int(m.details.ScrollPercent()*100))
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return ""
}

// formatSizeKB renders dpkg's Installed-Size, which is in kibibytes.
func formatSizeKB(s string) string {
	kb, ok := parseUint(s)
	if !ok {
		return ""
	}
	return humanBytes(kb * 1024)
}

// formatSizeBytes renders apt's Size field, which is in bytes.
func formatSizeBytes(s string) string {
	b, ok := parseUint(s)
	if !ok {
		return ""
	}
	return humanBytes(b)
}

func parseUint(s string) (uint64, bool) {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0, false
	}
	var n uint64
	for _, r := range s {
		if r < '0' || r > '9' {
			return 0, false
		}
		n = n*10 + uint64(r-'0')
	}
	return n, true
}

// humanBytes renders a byte count in the units an administrator reads sizes in.
func humanBytes(n uint64) string {
	const unit = 1024
	if n < unit {
		return fmt.Sprintf("%d B", n)
	}

	value := float64(n)
	units := []string{"kB", "MB", "GB", "TB"}
	for _, u := range units {
		value /= unit
		if value < unit {
			if value < 10 {
				return fmt.Sprintf("%.1f %s", value, u)
			}
			return fmt.Sprintf("%.0f %s", value, u)
		}
	}
	return fmt.Sprintf("%.0f TB", value)
}
