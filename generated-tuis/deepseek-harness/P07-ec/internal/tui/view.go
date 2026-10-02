package tui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"

	"toolg/internal/conflict"
	"toolg/internal/gitx"
)

// View renders the current screen.
func (m Model) View() string {
	if m.err != nil && m.repo == "" {
		return m.viewError()
	}
	switch m.screen {
	case screenMerge:
		return m.viewMerge()
	case screenCommit:
		return m.viewCommit()
	case screenHistory:
		return m.viewHistory()
	case screenHelp:
		return m.viewHelp()
	default:
		return m.viewFiles()
	}
}

func (m Model) viewError() string {
	w, _ := norm(m.width, m.height)
	var b strings.Builder
	b.WriteString(titleStyle.Render(padRunes(" toolg ", w)))
	b.WriteString("\n\n")
	b.WriteString(statusErrStyle.Render("  " + m.err.Error()))
	b.WriteString("\n\n")
	b.WriteString(keysStyle.Render(padRunes("q quit", w)))
	return b.String()
}

// viewMerge renders the three-way merge layout: ours / result / theirs panels
// side by side, all visible on one screen.
func (m Model) viewMerge() string {
	W, H := norm(m.width, m.height)

	// Vertical budget: title, progress, status, and key bars.
	const chrome = 4
	panelH := H - chrome
	if panelH < 3 {
		panelH = 3
	}

	innerW := (W - 8) / 3
	if innerW < 8 {
		innerW = 8
	}

	oursLines := m.sideLines(sideOurs, innerW)
	resLines := m.sideLines(sideResult, innerW)
	theirLines := m.sideLines(sideTheirs, innerW)

	oursPanel := renderPanel(" OURS ("+m.sideLabel(sideOurs)+") ", oursLines, innerW, panelH, m.scroll[sideOurs], oursBorder)
	resPanel := renderPanel(" RESULT ", resLines, innerW, panelH, m.scroll[sideResult], resultBorder)
	theirPanel := renderPanel(" THEIRS ("+m.sideLabel(sideTheirs)+") ", theirLines, innerW, panelH, m.scroll[sideTheirs], theirsBorder)

	row := lipgloss.JoinHorizontal(lipgloss.Top, oursPanel, resPanel, theirPanel)

	title := m.titleText()
	progress := m.progressText()

	var b strings.Builder
	b.WriteString(titleStyle.Render(padRunes(title, W)))
	b.WriteString("\n")
	b.WriteString(headerStyle.Render(padRunes(progress, W)))
	b.WriteString("\n")
	b.WriteString(row)
	b.WriteString("\n")
	b.WriteString(m.statusBar())
	b.WriteString("\n")
	b.WriteString(keysStyle.Render(padRunes(keyHint(screenMerge), W)))
	return b.String()
}

// titleText builds the top bar text.
func (m Model) titleText() string {
	merge := "no merge"
	if m.repo != "" && gitx.IsMerge(m.repo) {
		merge = "merge in progress"
	}
	p := m.relPath
	if p == "" {
		p = "no file"
	}
	return " toolg · " + p + " · " + merge + " "
}

// progressText builds the current-conflict / resolution line.
func (m Model) progressText() string {
	if m.cf == nil {
		return " no file open "
	}
	if !m.cf.HasConflicts() {
		return " no conflict markers in this file "
	}
	res := m.currentResolution().String()
	txt := " conflict " + itoa(m.current+1) + "/" + itoa(m.cf.ConflictCount()) + " · resolution: " + res
	if m.unsaved {
		txt += " · unsaved"
	}
	focusName := []string{"ours", "result", "theirs"}[m.focus]
	txt += " · focus: " + focusName
	return txt
}

// sideLabel returns the branch label for a panel.
func (m Model) sideLabel(side int) string {
	ours, theirs := "HEAD", "incoming"
	if m.cf != nil {
		for _, c := range m.cf.Chunks {
			if c.Kind == conflict.KindConflict {
				if c.OursLabel != "" {
					ours = c.OursLabel
				}
				if c.TheirsLabel != "" {
					theirs = c.TheirsLabel
				}
				break
			}
		}
	}
	switch side {
	case sideOurs:
		return ours
	case sideTheirs:
		return theirs
	default:
		return "result"
	}
}

// sideLines renders one side of the three-way view with per-region styling.
func (m Model) sideLines(side, width int) []string {
	if m.cf == nil {
		return []string{}
	}
	var out []string
	confNum := 0
	for _, c := range m.cf.Chunks {
		if c.Kind == conflict.KindConflict {
			current := confNum == m.current
			lines := m.chunkSideLines(c, side)
			st := conflictStyle(side, current)
			if side == sideResult && !c.Resolved() {
				st = unresolvedStyle(current)
			}
			for j, l := range lines {
				prefix := "  "
				if current && j == 0 {
					prefix = "▸ "
				}
				out = append(out, st.Render(padRunes(prefix+l, width)))
			}
			confNum++
			continue
		}
		for _, l := range c.Lines {
			out = append(out, plainStyle.Render(padRunes(l, width)))
		}
	}
	return out
}

// renderPanel clips and pads a side's lines and wraps them in a bordered box.
func renderPanel(title string, lines []string, innerW, height, scroll int, style lipgloss.Style) string {
	visible := clipLines(lines, scroll, height)
	for len(visible) < height {
		visible = append(visible, strings.Repeat(" ", innerW))
	}
	inner := padRunes(title, innerW) + "\n" + strings.Join(visible, "\n")
	return style.Render(inner)
}

// statusBar renders the feedback line.
func (m Model) statusBar() string {
	st := statusOkStyle
	if m.statusErr {
		st = statusErrStyle
	}
	return st.Render(padRunes(m.status, m.width))
}

func (m Model) viewFiles() string {
	W, H := norm(m.width, m.height)
	var b strings.Builder
	b.WriteString(titleStyle.Render(padRunes(" toolg — resolve merge conflicts ", W)))
	b.WriteString("\n")
	b.WriteString(headerStyle.Render(padRunes(" conflicted files ("+itoa(len(m.files))+") ", W)))
	b.WriteString("\n")
	b.WriteString("\n")

	avail := H - 5
	if avail < 1 {
		avail = 1
	}
	if len(m.files) == 0 {
		b.WriteString(infoStyle.Render(padRunes("  (no conflicted files — press r to refresh)", W)))
		b.WriteString("\n")
	} else {
		for i, f := range m.files {
			if i >= avail {
				break
			}
			marker := "  "
			st := fileUnselectedStyle
			if i == m.curFile {
				marker = "▸ "
				st = fileSelectedStyle
			}
			b.WriteString(st.Render(padRunes(marker+f, W)))
			b.WriteString("\n")
		}
	}
	b.WriteString("\n")
	b.WriteString(m.statusBar())
	b.WriteString("\n")
	b.WriteString(keysStyle.Render(padRunes(keyHint(screenFiles), W)))
	return b.String()
}

func (m Model) viewCommit() string {
	W, _ := norm(m.width, m.height)
	var b strings.Builder
	b.WriteString(titleStyle.Render(padRunes(" commit merge result ", W)))
	b.WriteString("\n")
	b.WriteString(headerStyle.Render(padRunes(" files: "+strings.Join(m.stagePreview(), ", "), W)))
	b.WriteString("\n")
	b.WriteString("\n")
	b.WriteString(infoStyle.Render(padRunes(" commit message (Enter to commit, Esc to cancel)", W)))
	b.WriteString("\n")
	b.WriteString(m.message.View())
	b.WriteString("\n")
	if m.commitErr != "" {
		b.WriteString(statusErrStyle.Render(padRunes(" error: "+m.commitErr, W)))
		b.WriteString("\n")
	}
	b.WriteString("\n")
	b.WriteString(m.statusBar())
	b.WriteString("\n")
	b.WriteString(keysStyle.Render(padRunes(keyHint(screenCommit), W)))
	return b.String()
}

func (m Model) viewHistory() string {
	W, H := norm(m.width, m.height)
	var b strings.Builder
	b.WriteString(titleStyle.Render(padRunes(" git commit history ", W)))
	b.WriteString("\n")
	avail := H - 4
	if avail < 1 {
		avail = 1
	}
	start := m.logScroll
	end := start + avail
	if end > len(m.logLines) {
		end = len(m.logLines)
	}
	for _, l := range m.logLines[start:end] {
		b.WriteString(infoStyle.Render(padRunes(" "+l, W)))
		b.WriteString("\n")
	}
	b.WriteString(keysStyle.Render(padRunes(keyHint(screenHistory), W)))
	return b.String()
}

func (m Model) viewHelp() string {
	W, _ := norm(m.width, m.height)
	var b strings.Builder
	b.WriteString(titleStyle.Render(padRunes(" toolg — keyboard reference ", W)))
	b.WriteString("\n")
	b.WriteString("\n")
	for _, bd := range helpBindings() {
		key := padRunes("  "+bd.keys, 24)
		line := helpKeyStyle.Render(key) + helpDescStyle.Render(" "+bd.desc)
		b.WriteString(line)
		b.WriteString("\n")
	}
	b.WriteString("\n")
	b.WriteString(keysStyle.Render(padRunes(keyHint(screenHelp), W)))
	return b.String()
}

// stagePreview returns the paths that a commit would stage.
func (m Model) stagePreview() []string {
	files, _ := gitx.ConflictedFiles(m.repo)
	return m.stageList(files)
}

// ---- small helpers ----

func norm(w, h int) (int, int) {
	if w < 24 {
		w = 24
	}
	if h < 8 {
		h = 8
	}
	return w, h
}

// padRunes truncates or pads s so that it is exactly n columns wide.
func padRunes(s string, n int) string {
	rs := []rune(s)
	for len(rs) < n {
		rs = append(rs, ' ')
	}
	if len(rs) > n {
		rs = rs[:n]
	}
	return string(rs)
}

// clipLines returns up to height lines starting at scroll.
func clipLines(lines []string, scroll, height int) []string {
	if scroll < 0 {
		scroll = 0
	}
	if scroll > len(lines) {
		scroll = len(lines)
	}
	end := scroll + height
	if end > len(lines) {
		end = len(lines)
	}
	return lines[scroll:end]
}
