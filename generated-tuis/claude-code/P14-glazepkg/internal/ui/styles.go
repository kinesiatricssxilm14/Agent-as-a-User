package ui

import "github.com/charmbracelet/lipgloss"

// Palette entries are chosen to stay legible on both dark and light terminals
// and to degrade gracefully where only 16 colours are available.
var (
	colBase    = lipgloss.AdaptiveColor{Light: "#1f2430", Dark: "#dfe3ec"}
	colDim     = lipgloss.AdaptiveColor{Light: "#6a7280", Dark: "#8b93a5"}
	colFaint   = lipgloss.AdaptiveColor{Light: "#9aa1ae", Dark: "#5d6473"}
	colAccent  = lipgloss.AdaptiveColor{Light: "#0b6bcb", Dark: "#63b3ff"}
	colBorder  = lipgloss.AdaptiveColor{Light: "#c3c9d5", Dark: "#3a4152"}
	colOK      = lipgloss.AdaptiveColor{Light: "#1a7f37", Dark: "#5fd08a"}
	colWarn    = lipgloss.AdaptiveColor{Light: "#9a6700", Dark: "#e3b341"}
	colErr     = lipgloss.AdaptiveColor{Light: "#b42318", Dark: "#ff7b72"}
	colSelBg   = lipgloss.AdaptiveColor{Light: "#dce8fb", Dark: "#2b3a55"}
	colHeadBg  = lipgloss.AdaptiveColor{Light: "#eef1f6", Dark: "#242a38"}
	colOverlay = lipgloss.AdaptiveColor{Light: "#ffffff", Dark: "#1b202b"}
)

type styles struct {
	title      lipgloss.Style
	tabActive  lipgloss.Style
	tabIdle    lipgloss.Style
	tabBar     lipgloss.Style
	panel      lipgloss.Style
	panelFocus lipgloss.Style
	panelTitle lipgloss.Style

	listHeader lipgloss.Style
	row        lipgloss.Style
	rowSel     lipgloss.Style
	rowCursor  lipgloss.Style
	marked     lipgloss.Style

	name    lipgloss.Style
	version lipgloss.Style
	latest  lipgloss.Style
	summary lipgloss.Style
	note    lipgloss.Style

	fieldKey lipgloss.Style
	fieldVal lipgloss.Style
	depName  lipgloss.Style

	statusOK    lipgloss.Style
	statusWarn  lipgloss.Style
	statusErr   lipgloss.Style
	statusBusy  lipgloss.Style
	statusPlain lipgloss.Style

	help     lipgloss.Style
	helpKey  lipgloss.Style
	helpDesc lipgloss.Style
	dim      lipgloss.Style
	accent   lipgloss.Style
	prompt   lipgloss.Style
	dialog   lipgloss.Style
	logLine  lipgloss.Style
	cmdLine  lipgloss.Style
}

func newStyles() styles {
	panel := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(colBorder).
		Padding(0, 1)

	return styles{
		title: lipgloss.NewStyle().Bold(true).Foreground(colAccent),
		tabActive: lipgloss.NewStyle().Bold(true).
			Foreground(lipgloss.AdaptiveColor{Light: "#ffffff", Dark: "#0b1020"}).
			Background(colAccent).Padding(0, 2),
		tabIdle: lipgloss.NewStyle().Foreground(colDim).
			Background(colHeadBg).Padding(0, 2),
		tabBar: lipgloss.NewStyle().Padding(0, 0),

		panel:      panel,
		panelFocus: panel.BorderForeground(colAccent),
		panelTitle: lipgloss.NewStyle().Bold(true).Foreground(colAccent),

		listHeader: lipgloss.NewStyle().Bold(true).Foreground(colDim).
			Background(colHeadBg),
		row:       lipgloss.NewStyle(),
		rowSel:    lipgloss.NewStyle().Background(colSelBg).Bold(true),
		rowCursor: lipgloss.NewStyle().Foreground(colAccent).Bold(true),
		marked:    lipgloss.NewStyle().Foreground(colWarn).Bold(true),

		name:    lipgloss.NewStyle().Foreground(colBase),
		version: lipgloss.NewStyle().Foreground(colOK),
		latest:  lipgloss.NewStyle().Foreground(colWarn),
		summary: lipgloss.NewStyle().Foreground(colDim),
		note:    lipgloss.NewStyle().Foreground(colFaint).Italic(true),

		fieldKey: lipgloss.NewStyle().Bold(true).Foreground(colAccent),
		fieldVal: lipgloss.NewStyle().Foreground(colBase),
		depName:  lipgloss.NewStyle().Foreground(colOK),

		statusOK:    lipgloss.NewStyle().Foreground(colOK).Bold(true),
		statusWarn:  lipgloss.NewStyle().Foreground(colWarn).Bold(true),
		statusErr:   lipgloss.NewStyle().Foreground(colErr).Bold(true),
		statusBusy:  lipgloss.NewStyle().Foreground(colAccent).Bold(true),
		statusPlain: lipgloss.NewStyle().Foreground(colDim),

		help:     lipgloss.NewStyle().Foreground(colDim),
		helpKey:  lipgloss.NewStyle().Bold(true).Foreground(colAccent),
		helpDesc: lipgloss.NewStyle().Foreground(colDim),
		dim:      lipgloss.NewStyle().Foreground(colFaint),
		accent:   lipgloss.NewStyle().Foreground(colAccent).Bold(true),
		prompt:   lipgloss.NewStyle().Bold(true).Foreground(colAccent),
		dialog: lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).
			BorderForeground(colAccent).Background(colOverlay).Padding(0, 1),
		logLine: lipgloss.NewStyle().Foreground(colDim),
		cmdLine: lipgloss.NewStyle().Foreground(colAccent),
	}
}
