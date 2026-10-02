package ui

import "github.com/charmbracelet/lipgloss"

var (
	// Color palette (works on 8/256-colour terminals).
	accent      = lipgloss.Color("86")  // green
	accentAlt   = lipgloss.Color("212") // pink
	subtle      = lipgloss.Color("241")
	muted       = lipgloss.Color("245")
	textColor   = lipgloss.Color("252")
	errorColor  = lipgloss.Color("203")
	warnColor   = lipgloss.Color("221")
	successCol  = lipgloss.Color("120")
	selectionBg = lipgloss.Color("235")

	titleStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("15")).
			Background(accent).
			Padding(0, 1).
			Bold(true)

	tabStyle = lipgloss.NewStyle().
			Padding(0, 1)

	activeTabStyle = tabStyle.Copy().
			Foreground(lipgloss.Color("0")).
			Background(accent).
			Bold(true)

	inactiveTabStyle = tabStyle.Copy().
				Foreground(muted)

	panelStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(subtle)

	panelTitleStyle = lipgloss.NewStyle().
			Foreground(muted).
			Bold(true)

	selectedRowStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("15")).
				Background(selectionBg).
				Bold(true)

	normalRowStyle = lipgloss.NewStyle().
			Foreground(textColor)

	versionStyle = lipgloss.NewStyle().
			Foreground(muted)

	headerStyle = lipgloss.NewStyle().
			Foreground(muted)

	helpTextStyle = lipgloss.NewStyle().
			Foreground(muted)

	promptStyle = lipgloss.NewStyle().
			Foreground(accentAlt).
			Bold(true)

	errorStyle = lipgloss.NewStyle().
			Foreground(errorColor).
			Bold(true)

	successStyle = lipgloss.NewStyle().
			Foreground(successCol).
			Bold(true)

	warnStyle = lipgloss.NewStyle().
			Foreground(warnColor).
			Bold(true)

	busyStyle = lipgloss.NewStyle().
			Foreground(accentAlt).
			Bold(true)

	fieldKeyStyle = lipgloss.NewStyle().
			Foreground(accent).
			Bold(true)

	colTitleListStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("0")).
				Background(accent).
				Bold(true)

	colTitleDetailStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("0")).
				Background(accentAlt).
				Bold(true)
)

// truncate shortens s to at most width runes, appending an ellipsis.
func truncate(s string, width int) string {
	if width <= 0 {
		return ""
	}
	runes := []rune(s)
	if len(runes) <= width {
		return s
	}
	if width <= 1 {
		return string(runes[:width])
	}
	return string(runes[:width-1]) + "…"
}
