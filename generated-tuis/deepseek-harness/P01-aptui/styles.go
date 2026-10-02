package main

import "github.com/charmbracelet/lipgloss"

var (
	accent     = lipgloss.Color("75")
	green      = lipgloss.Color("42")
	yellow     = lipgloss.Color("220")
	red        = lipgloss.Color("203")
	cyan       = lipgloss.Color("80")
	dimGray    = lipgloss.Color("240")
	brightGray = lipgloss.Color("250")
)

var (
	titleStyle       = lipgloss.NewStyle().Bold(true).Foreground(accent)
	headerBarStyle   = lipgloss.NewStyle().Background(lipgloss.Color("24"))
	headerRightStyle = lipgloss.NewStyle().Foreground(brightGray).Bold(true)

	searchStyle      = lipgloss.NewStyle().Foreground(brightGray)
	searchFocusStyle = lipgloss.NewStyle().Foreground(accent).Bold(true)
	hintStyle        = lipgloss.NewStyle().Foreground(dimGray)

	statusStyle     = lipgloss.NewStyle().Foreground(brightGray)
	statusError     = lipgloss.NewStyle().Foreground(red).Bold(true)
	statusOk        = lipgloss.NewStyle().Foreground(green).Bold(true)
	confirmBarStyle = lipgloss.NewStyle().Foreground(yellow).Background(lipgloss.Color("235")).Bold(true)
	runningStyle    = lipgloss.NewStyle().Foreground(cyan)

	helpBarStyle = lipgloss.NewStyle().Foreground(dimGray).Background(lipgloss.Color("236"))
	keyStyle     = lipgloss.NewStyle().Foreground(cyan).Bold(true)
	dimStyle     = lipgloss.NewStyle().Foreground(dimGray)

	spinnerStyle = lipgloss.NewStyle().Foreground(accent)

	detailsLabelStyle = lipgloss.NewStyle().Bold(true).Foreground(accent)
	depBulletStyle    = lipgloss.NewStyle().Foreground(dimGray)
	depStyle          = lipgloss.NewStyle().Foreground(brightGray)

	glyphInstalledStyle  = lipgloss.NewStyle().Foreground(green)
	glyphAvailableStyle  = lipgloss.NewStyle().Foreground(dimGray)
	glyphUpgradableStyle = lipgloss.NewStyle().Foreground(yellow)
	namePlainStyle       = lipgloss.NewStyle().Foreground(brightGray)
	nameInstalledStyle   = lipgloss.NewStyle().Foreground(brightGray)
	nameUpgradableStyle  = lipgloss.NewStyle().Foreground(yellow)
	versionDimStyle      = lipgloss.NewStyle().Foreground(dimGray)
	listSelectedStyle    = lipgloss.NewStyle().Foreground(brightGray).Background(lipgloss.Color("24")).Bold(true)

	separatorStyle = lipgloss.NewStyle().Foreground(dimGray)

	helpBoxStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(accent).
			Padding(0, 1).
			Foreground(brightGray).
			Background(lipgloss.Color("235"))
)
