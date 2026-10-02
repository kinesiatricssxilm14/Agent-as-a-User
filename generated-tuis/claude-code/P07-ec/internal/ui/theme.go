package ui

import "github.com/charmbracelet/lipgloss"

// Palette entries are adaptive so the interface stays readable on both light
// and dark terminals. Conflict sides are distinguished by background colour
// rather than only by foreground, because the specification requires a
// distinguishing background highlight for conflict text.
var (
	colFg       = lipgloss.AdaptiveColor{Light: "#1c1c1c", Dark: "#e4e4e4"}
	colDim      = lipgloss.AdaptiveColor{Light: "#6c6c6c", Dark: "#8a8a8a"}
	colFaint    = lipgloss.AdaptiveColor{Light: "#9e9e9e", Dark: "#5f5f5f"}
	colAccent   = lipgloss.AdaptiveColor{Light: "#005fd7", Dark: "#5fafff"}
	colBorder   = lipgloss.AdaptiveColor{Light: "#b2b2b2", Dark: "#4e4e4e"}
	colBorderHi = lipgloss.AdaptiveColor{Light: "#005fd7", Dark: "#5fafff"}

	// Side identity colours: ours is blue, theirs is magenta, base is amber.
	colOurs   = lipgloss.AdaptiveColor{Light: "#00448f", Dark: "#8ec7ff"}
	colTheirs = lipgloss.AdaptiveColor{Light: "#7a005f", Dark: "#ffa0e8"}
	colBase   = lipgloss.AdaptiveColor{Light: "#7a4b00", Dark: "#ffd08a"}

	// Conflict-side backgrounds. The "cur" variants mark the block under the
	// cursor and are deliberately stronger than the others.
	bgOurs      = lipgloss.AdaptiveColor{Light: "#cfe4ff", Dark: "#1f3355"}
	bgOursCur   = lipgloss.AdaptiveColor{Light: "#a8ccff", Dark: "#2f5288"}
	bgTheirs    = lipgloss.AdaptiveColor{Light: "#ffd6f5", Dark: "#4a1f42"}
	bgTheirsCur = lipgloss.AdaptiveColor{Light: "#ffb3ec", Dark: "#75326a"}
	bgBase      = lipgloss.AdaptiveColor{Light: "#ffe9c7", Dark: "#463314"}
	bgBaseCur   = lipgloss.AdaptiveColor{Light: "#ffd79a", Dark: "#6b4a13"}

	// Result backgrounds encode resolution state at a glance.
	bgResolved      = lipgloss.AdaptiveColor{Light: "#c9f0cd", Dark: "#1d3d24"}
	bgResolvedCur   = lipgloss.AdaptiveColor{Light: "#a2e6ab", Dark: "#2b5c36"}
	bgUnresolved    = lipgloss.AdaptiveColor{Light: "#ffd7d3", Dark: "#4d2320"}
	bgUnresolvedCur = lipgloss.AdaptiveColor{Light: "#ffb4ad", Dark: "#7a3630"}

	colOk     = lipgloss.AdaptiveColor{Light: "#006b1f", Dark: "#79e08a"}
	colWarn   = lipgloss.AdaptiveColor{Light: "#8a5300", Dark: "#ffc861"}
	colErr    = lipgloss.AdaptiveColor{Light: "#a60000", Dark: "#ff8a80"}
	colOnDark = lipgloss.Color("#ffffff")
)

type theme struct {
	// header
	title     lipgloss.Style
	crumb     lipgloss.Style
	badgeOk   lipgloss.Style
	badgeWarn lipgloss.Style
	badgeErr  lipgloss.Style
	badgeInfo lipgloss.Style

	// panels
	panel       lipgloss.Style
	panelActive lipgloss.Style
	panelTitle  lipgloss.Style
	gutter      lipgloss.Style
	gutterCur   lipgloss.Style

	// text
	context lipgloss.Style
	filler  lipgloss.Style
	dim     lipgloss.Style
	faint   lipgloss.Style
	accent  lipgloss.Style
	ok      lipgloss.Style
	warn    lipgloss.Style
	err     lipgloss.Style
	bold    lipgloss.Style

	// footer
	statusOk   lipgloss.Style
	statusWarn lipgloss.Style
	statusErr  lipgloss.Style
	keyName    lipgloss.Style
	keyDesc    lipgloss.Style
	prompt     lipgloss.Style
}

func newTheme() *theme {
	base := lipgloss.NewStyle()
	return &theme{
		title: base.Bold(true).Foreground(colOnDark).Background(colAccent).Padding(0, 1),
		crumb: base.Foreground(colFg),

		badgeOk:   base.Foreground(colOnDark).Background(colOk).Padding(0, 1).Bold(true),
		badgeWarn: base.Foreground(colOnDark).Background(colWarn).Padding(0, 1).Bold(true),
		badgeErr:  base.Foreground(colOnDark).Background(colErr).Padding(0, 1).Bold(true),
		badgeInfo: base.Foreground(colOnDark).Background(colDim).Padding(0, 1),

		panel:       base.Border(lipgloss.RoundedBorder()).BorderForeground(colBorder),
		panelActive: base.Border(lipgloss.RoundedBorder()).BorderForeground(colBorderHi),
		panelTitle:  base.Bold(true),
		gutter:      base.Foreground(colFaint),
		gutterCur:   base.Foreground(colAccent).Bold(true),

		context: base.Foreground(colFg),
		filler:  base.Foreground(colFaint),
		dim:     base.Foreground(colDim),
		faint:   base.Foreground(colFaint),
		accent:  base.Foreground(colAccent).Bold(true),
		ok:      base.Foreground(colOk),
		warn:    base.Foreground(colWarn),
		err:     base.Foreground(colErr),
		bold:    base.Bold(true),

		statusOk:   base.Foreground(colOk),
		statusWarn: base.Foreground(colWarn),
		statusErr:  base.Foreground(colErr).Bold(true),
		keyName:    base.Foreground(colAccent).Bold(true),
		keyDesc:    base.Foreground(colDim),
		prompt:     base.Foreground(colOnDark).Background(colAccent).Bold(true).Padding(0, 1),
	}
}

// sideStyle returns the text style for a conflict side, given whether the row
// belongs to the block under the cursor.
func (t *theme) sideStyle(s side, current bool) lipgloss.Style {
	st := lipgloss.NewStyle()
	switch s {
	case sideOurs:
		if current {
			return st.Foreground(colFg).Background(bgOursCur)
		}
		return st.Foreground(colFg).Background(bgOurs)
	case sideTheirs:
		if current {
			return st.Foreground(colFg).Background(bgTheirsCur)
		}
		return st.Foreground(colFg).Background(bgTheirs)
	case sideBase:
		if current {
			return st.Foreground(colFg).Background(bgBaseCur)
		}
		return st.Foreground(colFg).Background(bgBase)
	}
	return st.Foreground(colFg)
}

// resultStyle returns the style for a result-panel row inside a conflict.
func (t *theme) resultStyle(resolved, current bool) lipgloss.Style {
	st := lipgloss.NewStyle().Foreground(colFg)
	switch {
	case resolved && current:
		return st.Background(bgResolvedCur)
	case resolved:
		return st.Background(bgResolved)
	case current:
		return st.Background(bgUnresolvedCur)
	default:
		return st.Background(bgUnresolved)
	}
}

// sideColor returns the identity colour used for a side's panel title.
func sideColor(s side) lipgloss.TerminalColor {
	switch s {
	case sideOurs:
		return colOurs
	case sideTheirs:
		return colTheirs
	case sideBase:
		return colBase
	default:
		return colAccent
	}
}
