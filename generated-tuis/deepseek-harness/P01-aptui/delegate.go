package main

import (
	"fmt"
	"io"

	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// packageDelegate renders each package row with a status glyph, name and
// version in aligned columns.
type packageDelegate struct{}

func (d packageDelegate) Height() int                               { return 1 }
func (d packageDelegate) Spacing() int                              { return 0 }
func (d packageDelegate) Update(msg tea.Msg, m *list.Model) tea.Cmd { return nil }

func (d packageDelegate) Render(w io.Writer, m list.Model, index int, item list.Item) {
	p, ok := item.(Package)
	if !ok {
		return
	}
	selected := index == m.Index()

	width := m.Width()
	if width < 12 {
		width = 12
	}
	nameW := clamp(width*2/5, 10, 48)
	verW := width - nameW - 4
	if verW < 4 {
		verW = 4
	}

	var glyphStyle, nameStyle lipgloss.Style
	switch p.Status {
	case StatusInstalled:
		glyphStyle, nameStyle = glyphInstalledStyle, nameInstalledStyle
	case StatusUpgradable:
		glyphStyle, nameStyle = glyphUpgradableStyle, nameUpgradableStyle
	default:
		glyphStyle, nameStyle = glyphAvailableStyle, namePlainStyle
	}

	row := fmt.Sprintf("%s %-*s %-*s",
		glyphStyle.Render(p.Status.Glyph()),
		nameW, nameStyle.Render(truncate(p.Name, nameW)),
		verW, versionDimStyle.Render(truncate(p.versionForList(), verW)),
	)

	if selected {
		row = listSelectedStyle.Width(width).Render(row)
	} else {
		row = lipgloss.NewStyle().Width(width).Render(row)
	}
	fmt.Fprint(w, row)
}
