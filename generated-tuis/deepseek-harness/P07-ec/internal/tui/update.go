package tui

import (
	tea "github.com/charmbracelet/bubbletea"

	"toolg/internal/conflict"
)

// Update handles all messages for the model.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.message.Width = clamp(msg.Width-8, 10, 200)
		return m, nil

	case tea.KeyMsg:
		// Ctrl+C always quits.
		if msg.Type == tea.KeyCtrlC {
			return m, tea.Quit
		}

		switch m.screen {
		case screenHelp:
			if keyIn(msg, "?", "h", "esc", "q") {
				m.screen = m.prevScreen
			}
			return m, nil

		case screenHistory:
			return m.updateHistory(msg)

		case screenCommit:
			return m.updateCommit(msg)

		case screenFiles:
			return m.updateFiles(msg)

		case screenMerge:
			return m.updateMerge(msg)
		}
	}

	return m, nil
}

func (m Model) updateFiles(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case keyIn(msg, "q", "esc"):
		return m, tea.Quit
	case keyIn(msg, "?", "h"):
		m.showHelp()
	case keyIn(msg, "up", "k"):
		if len(m.files) > 0 {
			m.curFile = (m.curFile - 1 + len(m.files)) % len(m.files)
		}
	case keyIn(msg, "down", "j"):
		if len(m.files) > 0 {
			m.curFile = (m.curFile + 1) % len(m.files)
		}
	case keyIn(msg, "enter", " "):
		if len(m.files) > 0 && m.curFile < len(m.files) {
			m.openFile(resolvePath(m.repo, m.files[m.curFile]))
		}
	case keyIn(msg, "r"):
		m.refreshFiles()
		m.setStatus("refreshed file list", false)
	}
	return m, nil
}

func (m Model) updateMerge(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case keyIn(msg, "q"):
		return m, tea.Quit
	case keyIn(msg, "?", "h"):
		m.showHelp()
	case keyIn(msg, "f"):
		m.refreshFiles()
		m.prevScreen = screenMerge
		m.screen = screenFiles
	case keyIn(msg, "esc"):
		// Return to the file list (or quit if nothing else to do).
		if len(m.files) > 0 {
			m.refreshFiles()
			m.screen = screenFiles
		}

	// Conflict navigation.
	case keyIn(msg, "up", "k"):
		m.prevConflict()
	case keyIn(msg, "down", "j"):
		m.nextConflict()

	// Panel focus (for scrolling).
	case keyIn(msg, "tab"):
		m.focus = (m.focus + 1) % 3
	case keyIn(msg, "shift+tab"):
		m.focus = (m.focus + 2) % 3

	// Scrolling the focused panel.
	case keyIn(msg, "pgup", "[", "ctrl+u"):
		m.scroll[m.focus] -= scrollStep
		m.clampScroll(m.focus)
	case keyIn(msg, "pgdown", "]", "ctrl+d"):
		m.scroll[m.focus] += scrollStep
		m.clampScroll(m.focus)
	case keyIn(msg, "home"):
		m.scroll[m.focus] = 0
	case keyIn(msg, "end"):
		m.scroll[m.focus] = 1 << 30
		m.clampScroll(m.focus)

	// Resolution strategies for the current conflict.
	case keyIn(msg, "1", "o"):
		m.applyResolution(conflict.ResolveOurs)
	case keyIn(msg, "2", "t"):
		m.applyResolution(conflict.ResolveTheirs)
	case keyIn(msg, "3", "b"):
		m.applyResolution(conflict.ResolveBoth)
	case keyIn(msg, "4", "x"):
		m.applyResolution(conflict.ResolveNone)
	case keyIn(msg, "0", "r"):
		m.resetResolution()

	// Resolve every conflict at once.
	case keyIn(msg, "O"):
		m.resolveAll(conflict.ResolveOurs)
	case keyIn(msg, "T"):
		m.resolveAll(conflict.ResolveTheirs)
	case keyIn(msg, "B"):
		m.resolveAll(conflict.ResolveBoth)

	// File operations.
	case keyIn(msg, "s", "ctrl+s"):
		m.save()
	case keyIn(msg, "c"):
		if m.repo == "" {
			m.setStatus("not inside a git repository", true)
			return m, nil
		}
		m.message.Focus()
		m.message.SetValue("")
		m.commitErr = ""
		m.committed = ""
		m.prevScreen = screenMerge
		m.screen = screenCommit
	case keyIn(msg, "g"):
		m.showHistory()
	}
	return m, nil
}

func (m Model) updateCommit(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.Type {
	case tea.KeyEsc:
		m.message.Blur()
		m.screen = m.prevScreen
		return m, nil
	case tea.KeyEnter:
		m.doCommit()
		if m.commitErr == "" && m.committed != "" {
			m.message.Blur()
			m.screen = m.prevScreen
		}
		return m, nil
	}
	var cmd tea.Cmd
	m.message, cmd = m.message.Update(msg)
	return m, cmd
}

func (m Model) updateHistory(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case keyIn(msg, "q"):
		return m, tea.Quit
	case keyIn(msg, "esc", "h", "g", "?"):
		m.screen = m.prevScreen
	case keyIn(msg, "up", "k"):
		if m.logScroll > 0 {
			m.logScroll--
		}
	case keyIn(msg, "down", "j"):
		if m.logScroll < len(m.logLines)-1 {
			m.logScroll++
		}
	case keyIn(msg, "pgup"):
		m.logScroll -= 5
		m.clampLogScroll()
	case keyIn(msg, "pgdown"):
		m.logScroll += 5
		m.clampLogScroll()
	}
	return m, nil
}

func (m *Model) resolveAll(r conflict.Resolution) {
	if m.cf == nil || !m.cf.HasConflicts() {
		return
	}
	m.cf.ResolveAll(r)
	m.unsaved = true
	m.setStatus("all conflicts resolved as: "+r.String(), false)
}

const scrollStep = 3

func (m *Model) clampScroll(side int) {
	max := 0
	if m.cf != nil {
		max = len(m.sideLinesForCount(side)) - 1
	}
	if max < 0 {
		max = 0
	}
	if m.scroll[side] < 0 {
		m.scroll[side] = 0
	}
	if m.scroll[side] > max {
		m.scroll[side] = max
	}
}

func (m *Model) clampLogScroll() {
	if m.logScroll < 0 {
		m.logScroll = 0
	}
	max := len(m.logLines) - 1
	if max < 0 {
		max = 0
	}
	if m.logScroll > max {
		m.logScroll = max
	}
}

// sideLinesForCount returns the unstyled lines for a side, used only for
// computing the maximum scroll position.
func (m *Model) sideLinesForCount(side int) []string {
	if m.cf == nil {
		return nil
	}
	var out []string
	for _, c := range m.cf.Chunks {
		out = append(out, m.chunkSideLines(c, side)...)
	}
	return out
}
