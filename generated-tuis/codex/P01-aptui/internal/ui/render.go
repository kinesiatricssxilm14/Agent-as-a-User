package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"

	"toola/internal/apt"
)

var (
	colorAccent = lipgloss.Color("39")
	colorGreen  = lipgloss.Color("42")
	colorYellow = lipgloss.Color("214")
	colorRed    = lipgloss.Color("196")
	colorMuted  = lipgloss.Color("241")

	headerStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("231")).
			Background(lipgloss.Color("24")).
			Padding(0, 1)
	panelStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("238"))
	titleStyle = lipgloss.NewStyle().Bold(true).Foreground(colorAccent)
	keyStyle   = lipgloss.NewStyle().Bold(true).Foreground(colorAccent)
	helpStyle  = lipgloss.NewStyle().Foreground(colorMuted).Padding(0, 1)
	errorStyle = lipgloss.NewStyle().Foreground(colorRed).Padding(0, 1)
	okStyle    = lipgloss.NewStyle().Foreground(colorGreen).Padding(0, 1)
)

func (m Model) renderHeader() string {
	title := " TOOLA  Debian Package Manager"
	counts := fmt.Sprintf(
		" %s  all:%d installed:%d available:%d upgradable:%d ",
		filterName(m.filter),
		len(m.packages),
		countInstalled(m.packages),
		len(m.packages)-countInstalled(m.packages),
		m.upgradableCount(),
	)
	space := max(1, m.width-lipgloss.Width(title)-lipgloss.Width(counts))
	return headerStyle.Width(max(1, m.width-2)).Render(title + strings.Repeat(" ", space) + counts)
}

func (m Model) renderList(width, height int) string {
	var lines []string
	searchLine := m.search.View()
	if !m.searching && m.search.Value() == "" {
		searchLine = keyStyle.Render("/") + " search name/keyword"
	}
	lines = append(lines, titleStyle.Render("Packages")+"  "+searchLine)
	lines = append(lines, strings.Repeat("─", max(1, width)))

	visible := max(1, height-3)
	if len(m.filtered) == 0 {
		lines = append(lines, lipgloss.NewStyle().Foreground(colorMuted).Render("No matching packages"))
	} else {
		end := min(len(m.filtered), m.offset+visible)
		for index := m.offset; index < end; index++ {
			pkg := m.filtered[index]
			cursor := "  "
			style := lipgloss.NewStyle()
			if index == m.cursor {
				cursor = "› "
				style = style.Bold(true).Foreground(lipgloss.Color("231")).Background(lipgloss.Color("24"))
			}
			marker := " "
			switch {
			case pkg.Upgradable:
				marker = lipgloss.NewStyle().Foreground(colorYellow).Render("↑")
			case pkg.Installed():
				marker = lipgloss.NewStyle().Foreground(colorGreen).Render("●")
			default:
				marker = lipgloss.NewStyle().Foreground(colorMuted).Render("○")
			}
			nameWidth := max(1, width-5)
			name := truncate(pkg.Name, nameWidth)
			line := cursor + marker + " " + name
			line += strings.Repeat(" ", max(0, width-lipgloss.Width(line)))
			lines = append(lines, style.Render(line))
		}
	}

	position := "0/0"
	if len(m.filtered) > 0 {
		position = fmt.Sprintf("%d/%d", m.cursor+1, len(m.filtered))
	}
	scroll := fmt.Sprintf("%s  ● installed  ○ available  ↑ upgrade", position)
	lines = append(lines, lipgloss.NewStyle().Foreground(colorMuted).Render(truncate(scroll, width)))
	return strings.Join(lines, "\n")
}

func (m Model) renderStatus() string {
	status := m.status
	style := okStyle
	if m.statusIsError {
		style = errorStyle
	}
	if m.busy {
		status = m.spinner.View() + " " + m.busyText + "…"
		if m.mutating {
			status += " please wait; apt operations are not interrupted"
		}
	}
	if m.confirm != opNone {
		status = m.status + "  " + keyStyle.Render("[y/Enter]") + " confirm  " + keyStyle.Render("[n/Esc]") + " cancel"
	}
	if m.outputText != "" {
		status += "  " + keyStyle.Render("[o]") + " jump details/output"
	}
	return style.Width(max(1, m.width-2)).Render(truncate(status, max(1, m.width-4)))
}

func (m Model) renderHelp() string {
	segments := []string{
		keyStyle.Render("↑/↓") + " select",
		keyStyle.Render("PgUp/PgDn") + " page",
		keyStyle.Render("Ctrl+U/D") + " details scroll",
		keyStyle.Render("/") + " search",
		keyStyle.Render("1") + " all",
		keyStyle.Render("2") + " installed",
		keyStyle.Render("3") + " available",
		keyStyle.Render("4") + " upgrades",
		keyStyle.Render("i") + " install",
		keyStyle.Render("x") + " remove",
		keyStyle.Render("u/U") + " upgrade one/all",
		keyStyle.Render("a") + " apt update",
		keyStyle.Render("r") + " refresh",
		keyStyle.Render("o") + " jump output",
		keyStyle.Render("q") + " quit",
	}
	maxWidth := max(20, m.width-4)
	var lines []string
	line := ""
	for _, segment := range segments {
		candidate := segment
		if line != "" {
			candidate = line + "  " + segment
		}
		if line != "" && lipgloss.Width(candidate) > maxWidth {
			lines = append(lines, line)
			line = segment
		} else {
			line = candidate
		}
	}
	if line != "" {
		lines = append(lines, line)
	}
	return strings.Join(lines, "\n")
}

func renderDetails(details apt.Details, width int) string {
	if details.Name == "" {
		return titleStyle.Render("Package details") + "\n\nSelect a package to inspect it."
	}

	state := "available"
	stateStyle := lipgloss.NewStyle().Foreground(colorMuted)
	if details.Upgradable {
		state = "installed · upgrade available"
		stateStyle = lipgloss.NewStyle().Foreground(colorYellow)
	} else if details.Installed {
		state = "installed"
		stateStyle = lipgloss.NewStyle().Foreground(colorGreen)
	}

	var builder strings.Builder
	builder.WriteString(titleStyle.Render(details.Name))
	builder.WriteString("  ")
	builder.WriteString(stateStyle.Render(state))
	builder.WriteString("\n\n")
	writeField(&builder, "Installed version", displayVersion(details.InstalledVersion))
	candidate := details.CandidateVersion
	if candidate == "" || candidate == "(none)" {
		candidate = details.Version
	}
	writeField(&builder, "Candidate version", displayVersion(candidate))
	writeField(&builder, "Architecture", details.Architecture)
	writeField(&builder, "Section", details.Section)
	writeField(&builder, "Maintainer", details.Maintainer)
	writeField(&builder, "Homepage", details.Homepage)

	builder.WriteString("\n")
	builder.WriteString(titleStyle.Render(fmt.Sprintf("Dependencies (%d)", len(details.Dependencies))))
	builder.WriteString("\n")
	if len(details.Dependencies) == 0 {
		builder.WriteString("  None\n")
	} else {
		for _, dependency := range details.Dependencies {
			builder.WriteString("  • ")
			builder.WriteString(dependency.Name)
			builder.WriteString("  ")
			builder.WriteString(lipgloss.NewStyle().Foreground(colorMuted).Render("[" + dependency.Kind + "]"))
			builder.WriteString("\n")
		}
	}

	builder.WriteString("\n")
	builder.WriteString(titleStyle.Render("Full description"))
	builder.WriteString("\n")
	builder.WriteString(details.Description)
	builder.WriteString("\n\n")
	builder.WriteString(lipgloss.NewStyle().Foreground(colorMuted).Render("Scroll: Ctrl+U/Ctrl+D or ←/→"))
	return lipgloss.NewStyle().Width(max(1, width)).Render(builder.String())
}

func writeField(builder *strings.Builder, label, value string) {
	if strings.TrimSpace(value) == "" {
		value = "—"
	}
	builder.WriteString(lipgloss.NewStyle().Bold(true).Render(label + ":"))
	builder.WriteString(" ")
	builder.WriteString(value)
	builder.WriteString("\n")
}

func displayVersion(value string) string {
	if value == "(none)" {
		return ""
	}
	return value
}

func countInstalled(packages []apt.Package) int {
	count := 0
	for _, pkg := range packages {
		if pkg.Installed() {
			count++
		}
	}
	return count
}

func filterName(filter packageFilter) string {
	switch filter {
	case filterInstalled:
		return "INSTALLED"
	case filterAvailable:
		return "AVAILABLE"
	case filterUpgradable:
		return "UPGRADABLE"
	default:
		return "ALL"
	}
}

func operationTitle(op operation, name string) string {
	switch op {
	case opInstall:
		return "Install " + name
	case opRemove:
		return "Remove " + name
	case opUpgrade:
		return "Upgrade " + name
	case opUpgradeAll:
		return "Upgrade all packages"
	case opUpdate:
		return "Update package indexes"
	default:
		return "Operation"
	}
}

func confirmationText(op operation, name string) string {
	switch op {
	case opInstall:
		return fmt.Sprintf("Install %q and its required dependencies?", name)
	case opRemove:
		return fmt.Sprintf("Remove %q? apt may also remove dependent packages.", name)
	case opUpgrade:
		return fmt.Sprintf("Upgrade %q to its candidate version?", name)
	case opUpgradeAll:
		return "Upgrade all currently upgradable packages?"
	case opUpdate:
		return "Download current package indexes with apt-get update?"
	default:
		return "Run operation?"
	}
}

func truncate(value string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(value) <= width {
		return value
	}
	if width == 1 {
		return "…"
	}
	runes := []rune(value)
	for len(runes) > 0 && lipgloss.Width(string(runes)) > width-1 {
		runes = runes[:len(runes)-1]
	}
	return string(runes) + "…"
}
