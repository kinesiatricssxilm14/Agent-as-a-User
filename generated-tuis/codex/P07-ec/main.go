package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"toolg/internal/gitops"
	"toolg/internal/mergefile"
)

var (
	colorAccent = lipgloss.Color("39")
	colorOurs   = lipgloss.Color("22")
	colorTheirs = lipgloss.Color("52")
	colorResult = lipgloss.Color("24")
	colorMuted  = lipgloss.Color("245")
	colorWarn   = lipgloss.Color("214")
	colorGood   = lipgloss.Color("42")
	border      = lipgloss.Border{Top: "─", Bottom: "─", Left: "│", Right: "│", TopLeft: "╭", TopRight: "╮", BottomLeft: "╰", BottomRight: "╯"}
)

type clearStatusMsg struct{ at time.Time }

type model struct {
	file                 *mergefile.File
	path, displayPath    string
	repo                 gitops.Repo
	mode                 os.FileMode
	width, height        int
	ours, result, theirs viewport.Model
	history              viewport.Model
	selected             int
	status               string
	statusErr            bool
	statusAt             time.Time
	commitInput          textinput.Model
	committing           bool
	showHelp             bool
}

func newModel(file *mergefile.File, path, displayPath string, mode os.FileMode, repo gitops.Repo) model {
	ti := textinput.New()
	ti.Placeholder = "Merge branch resolution"
	ti.Prompt = "Commit message: "
	ti.CharLimit = 200
	m := model{file: file, path: path, displayPath: displayPath, repo: repo, mode: mode, commitInput: ti}
	m.ours, m.result, m.theirs, m.history = viewport.New(10, 5), viewport.New(10, 5), viewport.New(10, 5), viewport.New(10, 4)
	m.refresh()
	return m
}

func (m model) Init() tea.Cmd { return textinput.Blink }

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmd tea.Cmd
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.resize()
		m.refresh()
		return m, nil
	case clearStatusMsg:
		if msg.at.Equal(m.statusAt) {
			m.status = ""
		}
		return m, nil
	case tea.KeyMsg:
		if m.committing {
			switch msg.String() {
			case "esc":
				m.committing = false
				m.commitInput.Blur()
				m.setStatus("Commit cancelled", false)
				return m, nil
			case "enter":
				message := strings.TrimSpace(m.commitInput.Value())
				if message == "" {
					m.setStatus("Commit message cannot be empty", true)
					return m, nil
				}
				out, err := m.repo.Commit(message)
				if err != nil {
					m.setStatus(err.Error(), true)
				} else {
					m.setStatus("Merge committed: "+firstLine(out), false)
				}
				m.committing = false
				m.commitInput.Blur()
				m.refresh()
				return m, nil
			}
			m.commitInput, cmd = m.commitInput.Update(msg)
			return m, cmd
		}

		switch msg.String() {
		case "q", "ctrl+c":
			return m, tea.Quit
		case "?":
			m.showHelp = !m.showHelp
			m.refresh()
			return m, nil
		case "left", "shift+tab", "k", "up":
			if m.selected > 0 {
				m.selected--
				m.refresh()
			} else {
				m.scrollAll(-1)
			}
			return m, nil
		case "right", "tab", "j", "down":
			if m.selected+1 < len(m.file.Conflicts) {
				m.selected++
				m.refresh()
			} else {
				m.scrollAll(1)
			}
			return m, nil
		case "pgup":
			m.scrollAll(-5)
			return m, nil
		case "pgdown":
			m.scrollAll(5)
			return m, nil
		case "home":
			m.selected = 0
			m.refresh()
			return m, nil
		case "end":
			if len(m.file.Conflicts) > 0 {
				m.selected = len(m.file.Conflicts) - 1
				m.refresh()
			}
			return m, nil
		case "o", "1":
			m.choose(mergefile.Ours)
			return m, nil
		case "t", "2":
			m.choose(mergefile.Theirs)
			return m, nil
		case "b", "3":
			m.choose(mergefile.Both)
			return m, nil
		case "n", "4", "delete", "backspace":
			m.choose(mergefile.None)
			return m, nil
		case "u":
			m.choose(mergefile.Unresolved)
			return m, nil
		case "O":
			m.chooseAll(mergefile.Ours)
			return m, nil
		case "T":
			m.chooseAll(mergefile.Theirs)
			return m, nil
		case "B":
			m.chooseAll(mergefile.Both)
			return m, nil
		case "N":
			m.chooseAll(mergefile.None)
			return m, nil
		case "w", "ctrl+s":
			m.write()
			return m, nil
		case "c":
			if !m.file.AllResolved() {
				m.setStatus("Resolve every conflict and write it before committing", true)
				return m, nil
			}
			m.write()
			if m.statusErr {
				return m, nil
			}
			m.committing = true
			m.commitInput.SetValue("")
			m.commitInput.Focus()
			return m, textinput.Blink
		case "r":
			f, err := mergefile.Read(m.path)
			if err != nil {
				m.setStatus("Reload failed: "+err.Error(), true)
			} else {
				m.file = f
				m.selected = 0
				m.setStatus("Reloaded from disk", false)
				m.refresh()
			}
			return m, nil
		}
	}
	return m, cmd
}

func (m *model) choose(c mergefile.Choice) {
	if len(m.file.Conflicts) == 0 {
		m.setStatus("No conflicts in this file", true)
		return
	}
	m.file.Conflicts[m.selected].Choice = c
	m.setStatus(fmt.Sprintf("Conflict %d → %s", m.selected+1, c), false)
	m.refresh()
}
func (m *model) chooseAll(c mergefile.Choice) {
	m.file.ResolveAll(c)
	m.setStatus("All conflicts → "+c.String(), false)
	m.refresh()
}
func (m *model) write() {
	if !m.file.AllResolved() {
		m.setStatus(fmt.Sprintf("Cannot write: %d conflict(s) unresolved", m.unresolved()), true)
		return
	}
	if err := m.file.Write(m.path, m.mode); err != nil {
		m.setStatus("Write failed: "+err.Error(), true)
		return
	}
	if err := m.repo.Add(m.displayPath); err != nil {
		m.setStatus("File written, but staging failed: "+err.Error(), true)
		return
	}
	m.setStatus("Saved and staged "+m.displayPath, false)
	m.refresh()
}
func (m *model) unresolved() int {
	n := 0
	for _, c := range m.file.Conflicts {
		if c.Choice == mergefile.Unresolved {
			n++
		}
	}
	return n
}
func (m *model) setStatus(s string, isErr bool) {
	m.status = s
	m.statusErr = isErr
	m.statusAt = time.Now()
}
func (m *model) scrollAll(delta int) {
	for _, v := range []*viewport.Model{&m.ours, &m.result, &m.theirs} {
		if delta < 0 {
			v.LineUp(-delta)
		} else {
			v.LineDown(delta)
		}
	}
}

func (m *model) resize() {
	w := max(20, (m.width-4)/3)
	mainH := max(5, m.height-12)
	historyH := 4
	if m.showHelp {
		mainH = max(4, m.height-18)
		historyH = 3
	}
	m.ours.Width, m.result.Width, m.theirs.Width = w-2, w-2, w-2
	m.ours.Height, m.result.Height, m.theirs.Height = mainH, mainH, mainH
	m.history.Width = max(20, m.width-2)
	m.history.Height = historyH
}

func (m *model) refresh() {
	if len(m.file.Conflicts) > 0 && m.selected >= len(m.file.Conflicts) {
		m.selected = len(m.file.Conflicts) - 1
	}
	m.ours.SetContent(m.panelContent("ours"))
	m.result.SetContent(m.panelContent("result"))
	m.theirs.SetContent(m.panelContent("theirs"))
	m.history.SetContent(m.repo.History(8))
}

func (m *model) panelContent(kind string) string {
	if len(m.file.Conflicts) == 0 {
		return "No conflict markers found.\n\nPress q to quit or r to reload."
	}
	var b strings.Builder
	for _, p := range m.file.Parts {
		if p.Conflict == nil {
			for _, line := range p.Text {
				b.WriteString(dimLine(line))
			}
			continue
		}
		c := p.Conflict
		selected := iConflictIndex(m.file, c) == m.selected
		prefix := "  "
		if selected {
			prefix = "▶ "
		}
		label := fmt.Sprintf("%sCONFLICT %d · %s", prefix, iConflictIndex(m.file, c)+1, c.Choice)
		b.WriteString(lipgloss.NewStyle().Bold(true).Foreground(colorAccent).Render(label) + "\n")
		var lines []string
		bg := colorResult
		switch kind {
		case "ours":
			lines = c.Ours
			bg = colorOurs
		case "theirs":
			lines = c.Theirs
			bg = colorTheirs
		default:
			switch c.Choice {
			case mergefile.Ours:
				lines = c.Ours
				bg = colorOurs
			case mergefile.Theirs:
				lines = c.Theirs
				bg = colorTheirs
			case mergefile.Both:
				lines = append(append([]string{}, c.Ours...), c.Theirs...)
			case mergefile.None:
				lines = []string{"∅ conflict removed\n"}
			default:
				lines = []string{" unresolved — choose o/t/b/n \n"}
			}
		}
		if len(lines) == 0 {
			lines = []string{"∅ empty side\n"}
		}
		style := lipgloss.NewStyle().Background(bg).Foreground(lipgloss.Color("255"))
		for _, line := range lines {
			b.WriteString(style.Render(strings.TrimSuffix(line, "\n")) + "\n")
		}
	}
	return b.String()
}

func iConflictIndex(f *mergefile.File, wanted *mergefile.Conflict) int {
	for i, c := range f.Conflicts {
		if c == wanted {
			return i
		}
	}
	return 0
}
func dimLine(line string) string {
	return lipgloss.NewStyle().Foreground(colorMuted).Render(strings.TrimSuffix(line, "\n")) + "\n"
}

func (m model) View() string {
	if m.width == 0 {
		return "Starting toolg…\n"
	}
	title := lipgloss.NewStyle().Bold(true).Foreground(colorAccent).Render("toolg")
	progress := fmt.Sprintf("%s  %s  conflict %d/%d  unresolved: %d  git: %s", title, m.displayPath, min(m.selected+1, max(1, len(m.file.Conflicts))), len(m.file.Conflicts), m.unresolved(), m.repo.Status(m.displayPath))
	panelW := max(20, (m.width-4)/3)
	panelStyle := lipgloss.NewStyle().Border(border).BorderForeground(colorMuted).Width(panelW - 2)
	activeStyle := panelStyle.BorderForeground(colorAccent)
	panel := func(name string, v viewport.Model, active bool) string {
		s := panelStyle
		if active {
			s = activeStyle
		}
		heading := lipgloss.NewStyle().Bold(true).Render(name)
		return s.Render(heading + "\n" + v.View())
	}
	body := lipgloss.JoinHorizontal(lipgloss.Top, panel("OURS", m.ours, false), " ", panel("RESULT", m.result, true), " ", panel("THEIRS", m.theirs, false))
	hist := lipgloss.NewStyle().Border(border).BorderForeground(colorMuted).Width(max(20, m.width-2)).Render(lipgloss.NewStyle().Bold(true).Render("RECENT HISTORY") + "\n" + m.history.View())
	footer := "←/→ or j/k conflict  o ours  t theirs  b both  n none  Shift+key all  w save+stage  c commit  r reload  ? help  q quit"
	if m.showHelp {
		footer = "KEYS\n  Navigation: ←/→, j/k, Tab/Shift+Tab; PgUp/PgDn scroll; Home/End jump\n  Resolution: o/1 ours, t/2 theirs, b/3 both, n/4 none, u undo; uppercase applies all\n  Git: w/Ctrl+S write and stage; c write, stage and commit; r reload; Esc cancels input; q quits\n" + footer
	}
	if m.committing {
		footer = m.commitInput.View() + "  [Enter commit · Esc cancel]"
	}
	if m.status != "" {
		st := lipgloss.NewStyle().Foreground(colorGood)
		if m.statusErr {
			st = st.Foreground(colorWarn)
		}
		footer = st.Render(m.status) + "\n" + footer
	}
	return progress + "\n" + body + "\n" + hist + "\n" + lipgloss.NewStyle().Foreground(colorMuted).Render(footer) + "\n"
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i]
	}
	return s
}
func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func main() {
	var repoDir string
	flag.StringVar(&repoDir, "C", "/bench/data/repo", "Git working directory")
	flag.StringVar(&repoDir, "repo", "/bench/data/repo", "Git working directory")
	flag.Usage = func() {
		fmt.Fprintf(flag.CommandLine.Output(), "Usage: toolg [-C repository] [conflict-file]\n\nKeyboard-driven three-way Git conflict resolver.\n")
		flag.PrintDefaults()
	}
	flag.Parse()
	name := "conflict.py"
	if flag.NArg() > 0 {
		name = flag.Arg(0)
	}
	if flag.NArg() > 1 {
		fatal("only one conflict file may be specified")
	}
	absRepo, err := filepath.Abs(repoDir)
	if err != nil {
		fatal(err.Error())
	}
	repo := gitops.Repo{Dir: absRepo}
	root, err := repo.Root()
	if err != nil {
		fatal(err.Error())
	}
	repo.Dir = root
	path := name
	if !filepath.IsAbs(path) {
		path = filepath.Join(absRepo, path)
	}
	path, err = filepath.Abs(path)
	if err != nil {
		fatal(err.Error())
	}
	rel, err := filepath.Rel(root, path)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		fatal("conflict file must be inside the Git repository")
	}
	info, err := os.Stat(path)
	if err != nil {
		fatal("open conflict file: " + err.Error())
	}
	f, err := mergefile.Read(path)
	if err != nil {
		fatal("parse conflict file: " + err.Error())
	}
	p := tea.NewProgram(newModel(f, path, filepath.ToSlash(rel), info.Mode().Perm(), repo), tea.WithAltScreen())
	if _, err := p.Run(); err != nil && err != io.EOF {
		fatal(err.Error())
	}
}
func fatal(s string) { fmt.Fprintln(os.Stderr, "toolg:", s); os.Exit(1) }
