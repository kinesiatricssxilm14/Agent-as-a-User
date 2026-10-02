package tui

import "github.com/charmbracelet/lipgloss"

// Central color palette. 256-color values are used for broad terminal
// compatibility inside minimal containers.

var (
	titleStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("15")).
			Background(lipgloss.Color("24"))

	headerStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("245"))

	infoStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("250"))

	statusOkStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("10"))

	statusErrStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("9"))

	keysStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("240"))

	helpKeyStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("39"))

	helpDescStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("250"))

	// plainStyle is used for non-conflict (context) lines in the merge panels.
	plainStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("244"))

	// Conflict-side styles: each side gets a distinguishing background.
	oursStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("19")).
			Foreground(lipgloss.Color("252"))

	theirsStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("88")).
			Foreground(lipgloss.Color("252"))

	resultStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("22")).
			Foreground(lipgloss.Color("252"))

	// Brighter variants for the currently selected conflict region.
	oursCurStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("27")).
			Foreground(lipgloss.Color("231")).
			Bold(true)

	theirsCurStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("160")).
			Foreground(lipgloss.Color("231")).
			Bold(true)

	resultCurStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("34")).
			Foreground(lipgloss.Color("231")).
			Bold(true)

	// resultUnresolvedStyle shows the still-raw conflict in the result panel.
	resultUnresolvedStyle = lipgloss.NewStyle().
				Background(lipgloss.Color("22")).
				Foreground(lipgloss.Color("245"))

	resultUnresolvedCurStyle = lipgloss.NewStyle().
					Background(lipgloss.Color("34")).
					Foreground(lipgloss.Color("252")).
					Bold(true)

	// Panel border colors, one per side.
	oursBorder = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("33"))

	theirsBorder = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("196"))

	resultBorder = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("40"))

	// Files list styles.
	fileSelectedStyle = lipgloss.NewStyle().
				Bold(true).
				Background(lipgloss.Color("24")).
				Foreground(lipgloss.Color("15"))

	fileUnselectedStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("250"))

	// Unresolved marker line inside result panel.
	unresolvedTextStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("245")).
				Italic(true)
)

// conflictStyle returns the style for a conflict region on the given side.
func conflictStyle(side int, current bool) lipgloss.Style {
	switch side {
	case sideOurs:
		if current {
			return oursCurStyle
		}
		return oursStyle
	case sideTheirs:
		if current {
			return theirsCurStyle
		}
		return theirsStyle
	default: // sideResult
		if current {
			return resultCurStyle
		}
		return resultStyle
	}
}

// unresolvedStyle returns the style used for an unresolved block in the result
// panel.
func unresolvedStyle(current bool) lipgloss.Style {
	if current {
		return resultUnresolvedCurStyle
	}
	return resultUnresolvedStyle
}
