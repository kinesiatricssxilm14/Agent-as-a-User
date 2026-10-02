package main

import "github.com/charmbracelet/lipgloss"

// Colours are chosen from the 256-colour cube with light/dark variants, so toola
// stays legible on either terminal background and degrades gracefully on
// terminals that only do 16 colours.
var (
	colAccent    = lipgloss.AdaptiveColor{Light: "27", Dark: "39"}   // blue
	colMuted     = lipgloss.AdaptiveColor{Light: "244", Dark: "245"} // grey
	colFaint     = lipgloss.AdaptiveColor{Light: "250", Dark: "240"} // borders
	colSuccess   = lipgloss.AdaptiveColor{Light: "28", Dark: "42"}   // green
	colWarn      = lipgloss.AdaptiveColor{Light: "166", Dark: "214"} // amber
	colError     = lipgloss.AdaptiveColor{Light: "160", Dark: "203"} // red
	colHighlight = lipgloss.AdaptiveColor{Light: "254", Dark: "236"} // selection bg
	colText      = lipgloss.AdaptiveColor{Light: "235", Dark: "252"}
)

// styles bundles every Lip Gloss style toola renders with.
type styles struct {
	Header      lipgloss.Style
	HeaderTitle lipgloss.Style
	HeaderCount lipgloss.Style
	HeaderWarn  lipgloss.Style

	SearchLabel lipgloss.Style
	SearchCount lipgloss.Style

	Pane        lipgloss.Style
	PaneFocused lipgloss.Style
	PaneTitle   lipgloss.Style

	ListRow         lipgloss.Style
	ListRowSelected lipgloss.Style
	ListName        lipgloss.Style
	ListVersion     lipgloss.Style
	ListSection     lipgloss.Style
	ListSynopsis    lipgloss.Style
	ListEmpty       lipgloss.Style

	MarkInstalled  lipgloss.Style
	MarkUpgradable lipgloss.Style
	MarkAvailable  lipgloss.Style
	MarkResidual   lipgloss.Style

	DetailName    lipgloss.Style
	DetailField   lipgloss.Style
	DetailValue   lipgloss.Style
	DetailSection lipgloss.Style
	DetailDep     lipgloss.Style
	DetailDepAlt  lipgloss.Style
	DetailText    lipgloss.Style

	LogLine   lipgloss.Style
	LogStderr lipgloss.Style
	LogCmd    lipgloss.Style

	StatusInfo    lipgloss.Style
	StatusSuccess lipgloss.Style
	StatusWarn    lipgloss.Style
	StatusError   lipgloss.Style
	StatusConfirm lipgloss.Style

	ScrollInfo lipgloss.Style
}

func newStyles() styles {
	var s styles

	s.Header = lipgloss.NewStyle()
	s.HeaderTitle = lipgloss.NewStyle().Bold(true).Foreground(colAccent)
	s.HeaderCount = lipgloss.NewStyle().Foreground(colMuted)
	s.HeaderWarn = lipgloss.NewStyle().Bold(true).Foreground(colWarn)

	s.SearchLabel = lipgloss.NewStyle().Bold(true).Foreground(colAccent)
	s.SearchCount = lipgloss.NewStyle().Foreground(colMuted)

	// Panes are bordered so the boundaries between list, details and log are
	// unambiguous; the focused pane's border is accented.
	s.Pane = lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(colFaint)
	s.PaneFocused = lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(colAccent)
	s.PaneTitle = lipgloss.NewStyle().Bold(true).Foreground(colAccent)

	s.ListRow = lipgloss.NewStyle()
	s.ListRowSelected = lipgloss.NewStyle().Background(colHighlight).Bold(true)
	s.ListName = lipgloss.NewStyle().Foreground(colText)
	s.ListVersion = lipgloss.NewStyle().Foreground(colMuted)
	s.ListSection = lipgloss.NewStyle().Foreground(colAccent)
	s.ListSynopsis = lipgloss.NewStyle().Foreground(colMuted)
	s.ListEmpty = lipgloss.NewStyle().Foreground(colMuted).Italic(true)

	s.MarkInstalled = lipgloss.NewStyle().Foreground(colSuccess).Bold(true)
	s.MarkUpgradable = lipgloss.NewStyle().Foreground(colWarn).Bold(true)
	s.MarkAvailable = lipgloss.NewStyle().Foreground(colFaint)
	s.MarkResidual = lipgloss.NewStyle().Foreground(colError)

	s.DetailName = lipgloss.NewStyle().Bold(true).Foreground(colAccent)
	s.DetailField = lipgloss.NewStyle().Foreground(colMuted)
	s.DetailValue = lipgloss.NewStyle().Foreground(colText)
	s.DetailSection = lipgloss.NewStyle().Bold(true).Foreground(colWarn)
	s.DetailDep = lipgloss.NewStyle().Foreground(colText)
	s.DetailDepAlt = lipgloss.NewStyle().Foreground(colMuted)
	s.DetailText = lipgloss.NewStyle().Foreground(colText)

	s.LogLine = lipgloss.NewStyle().Foreground(colText)
	s.LogStderr = lipgloss.NewStyle().Foreground(colWarn)
	s.LogCmd = lipgloss.NewStyle().Bold(true).Foreground(colAccent)

	s.StatusInfo = lipgloss.NewStyle().Foreground(colMuted)
	s.StatusSuccess = lipgloss.NewStyle().Bold(true).Foreground(colSuccess)
	s.StatusWarn = lipgloss.NewStyle().Bold(true).Foreground(colWarn)
	s.StatusError = lipgloss.NewStyle().Bold(true).Foreground(colError)
	s.StatusConfirm = lipgloss.NewStyle().Bold(true).Foreground(colAccent)

	s.ScrollInfo = lipgloss.NewStyle().Foreground(colMuted)

	return s
}

// statusStyle maps a level to its style.
func (s styles) statusStyle(level statusLevel) lipgloss.Style {
	switch level {
	case statusSuccess:
		return s.StatusSuccess
	case statusWarn:
		return s.StatusWarn
	case statusError:
		return s.StatusError
	default:
		return s.StatusInfo
	}
}

// pane returns the bordered pane style, accented when focused.
func (s styles) pane(focused bool) lipgloss.Style {
	if focused {
		return s.PaneFocused
	}
	return s.Pane
}
