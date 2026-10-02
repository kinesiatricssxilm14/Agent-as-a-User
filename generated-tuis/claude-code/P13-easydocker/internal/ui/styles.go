package ui

import "github.com/charmbracelet/lipgloss"

// Palette used across the interface. Colours are given as adaptive pairs so
// toolm stays readable on light and dark terminals alike.
var (
	colAccent    = lipgloss.AdaptiveColor{Light: "#0B5FA5", Dark: "#7DC4FF"}
	colAccentAlt = lipgloss.AdaptiveColor{Light: "#7A3E9D", Dark: "#C7A0F5"}
	colText      = lipgloss.AdaptiveColor{Light: "#1C1C1C", Dark: "#E6E6E6"}
	colMuted     = lipgloss.AdaptiveColor{Light: "#6B6B6B", Dark: "#9A9A9A"}
	colFaint     = lipgloss.AdaptiveColor{Light: "#8C8C8C", Dark: "#6C6C6C"}
	colOK        = lipgloss.AdaptiveColor{Light: "#127A2E", Dark: "#5FD787"}
	colWarn      = lipgloss.AdaptiveColor{Light: "#9A6300", Dark: "#FFD75F"}
	colErr       = lipgloss.AdaptiveColor{Light: "#B21A1A", Dark: "#FF8787"}
	colBorder    = lipgloss.AdaptiveColor{Light: "#B9C2CC", Dark: "#3F4A55"}
	colSelBG     = lipgloss.AdaptiveColor{Light: "#CFE4FA", Dark: "#274058"}
)

// Styles bundles every lipgloss style the views use.
type Styles struct {
	Title       lipgloss.Style
	TitleDim    lipgloss.Style
	TabActive   lipgloss.Style
	TabInactive lipgloss.Style
	TabBar      lipgloss.Style

	PanelTitle       lipgloss.Style
	PanelTitleActive lipgloss.Style
	Panel            lipgloss.Style
	PanelFocused     lipgloss.Style

	TableHeader   lipgloss.Style
	Row           lipgloss.Style
	RowSelected   lipgloss.Style
	RowCursorMark lipgloss.Style

	FieldKey   lipgloss.Style
	FieldValue lipgloss.Style
	SectionHdr lipgloss.Style

	StatusBar lipgloss.Style
	KeyCap    lipgloss.Style
	KeyDesc   lipgloss.Style
	Muted     lipgloss.Style
	Faint     lipgloss.Style
	OK        lipgloss.Style
	Warn      lipgloss.Style
	Err       lipgloss.Style
	Accent    lipgloss.Style
	Match     lipgloss.Style
	Scroll    lipgloss.Style
	Spinner   lipgloss.Style
	LogLine   lipgloss.Style
	LogGutter lipgloss.Style
}

// NewStyles builds the style set.
func NewStyles() Styles {
	base := lipgloss.NewStyle()
	return Styles{
		Title:       base.Bold(true).Foreground(colAccent),
		TitleDim:    base.Foreground(colMuted),
		TabActive:   base.Bold(true).Foreground(lipgloss.Color("15")).Background(colAccent).Padding(0, 2),
		TabInactive: base.Foreground(colMuted).Padding(0, 2),
		TabBar:      base.Padding(0, 0),

		PanelTitle:       base.Bold(true).Foreground(colMuted),
		PanelTitleActive: base.Bold(true).Foreground(colAccent),
		Panel:            base.Border(lipgloss.RoundedBorder()).BorderForeground(colBorder),
		PanelFocused:     base.Border(lipgloss.RoundedBorder()).BorderForeground(colAccent),

		TableHeader:   base.Bold(true).Foreground(colAccentAlt).Underline(true),
		Row:           base.Foreground(colText),
		RowSelected:   base.Bold(true).Foreground(colText).Background(colSelBG),
		RowCursorMark: base.Bold(true).Foreground(colAccent),

		FieldKey:   base.Bold(true).Foreground(colMuted),
		FieldValue: base.Foreground(colText),
		SectionHdr: base.Bold(true).Foreground(colAccentAlt),

		StatusBar: base.Foreground(colMuted),
		KeyCap:    base.Bold(true).Foreground(colAccent),
		KeyDesc:   base.Foreground(colMuted),
		Muted:     base.Foreground(colMuted),
		Faint:     base.Foreground(colFaint),
		OK:        base.Foreground(colOK),
		Warn:      base.Foreground(colWarn),
		Err:       base.Foreground(colErr),
		Accent:    base.Foreground(colAccent),
		Match:     base.Bold(true).Foreground(lipgloss.Color("0")).Background(colWarn),
		Scroll:    base.Foreground(colFaint),
		Spinner:   base.Foreground(colAccent),
		LogLine:   base.Foreground(colText),
		LogGutter: base.Foreground(colFaint),
	}
}
