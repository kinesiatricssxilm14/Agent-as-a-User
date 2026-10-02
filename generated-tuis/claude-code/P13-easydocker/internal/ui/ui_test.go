package ui

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"toolm/internal/docker"

	"github.com/charmbracelet/lipgloss"

	tea "github.com/charmbracelet/bubbletea"
)

// fakeClient serves a fixed data set so the UI can be driven deterministically.
type fakeClient struct {
	containers []docker.Container
	images     []docker.Image
	networks   []docker.Network
	volumes    []docker.Volume
	logs       map[string]string

	failContainers bool
	inspectErr     bool
}

func (f *fakeClient) Endpoint() string { return "unix:///test/docker.sock" }

func (f *fakeClient) Version(context.Context) (string, error) { return "24.0.7 (API 1.43)", nil }

func (f *fakeClient) Containers(context.Context) ([]docker.Container, error) {
	if f.failContainers {
		return nil, fmt.Errorf("connection refused")
	}
	return f.containers, nil
}

func (f *fakeClient) ContainerInspect(_ context.Context, id string) (*docker.ContainerDetails, error) {
	if f.inspectErr {
		return nil, fmt.Errorf("no such container")
	}
	for _, c := range f.containers {
		if c.ID == id {
			return &docker.ContainerDetails{
				ID:      c.ID,
				Name:    "/" + c.Name(),
				Path:    "nginx",
				Args:    []string{"-g", "daemon off;"},
				Created: "2024-05-01T10:00:00Z",
				Image:   c.ImageID,
				State:   &docker.ContainerState{Status: c.StateLabel(), Running: c.IsRunning(), Pid: 42},
				Config:  &docker.Config{Image: c.Image, Cmd: docker.StringList{"nginx", "-g", "daemon off;"}},
				NetworkSettings: &docker.NetworkSettings{
					Ports: map[string][]docker.PortBinding{
						"80/tcp": {{HostIP: "0.0.0.0", HostPort: "8080"}},
					},
				},
			}, nil
		}
	}
	return nil, fmt.Errorf("no such container: %s", id)
}

func (f *fakeClient) ContainerLogs(_ context.Context, id string) (string, error) {
	if s, ok := f.logs[id]; ok {
		return s, nil
	}
	return "", nil
}

func (f *fakeClient) Images(context.Context) ([]docker.Image, error) { return f.images, nil }

func (f *fakeClient) ImageInspect(_ context.Context, ref string) (*docker.ImageDetails, error) {
	if f.inspectErr {
		return nil, fmt.Errorf("no such image")
	}
	for _, img := range f.images {
		repo, tag := imageRepoTag(img)
		if ref == repo+":"+tag || ref == img.ID || ref == docker.ShortID(img.ID) {
			return &docker.ImageDetails{
				ID:       img.ID,
				RepoTags: img.RepoTags,
				Size:     img.Size,
				Os:       "linux",
				Created:  "2024-04-01T08:30:00Z",
				Config:   &docker.Config{Cmd: docker.StringList{"nginx", "-g", "daemon off;"}},
			}, nil
		}
	}
	return nil, fmt.Errorf("no such image: %s", ref)
}

func (f *fakeClient) Networks(context.Context) ([]docker.Network, error) { return f.networks, nil }

func (f *fakeClient) NetworkInspect(_ context.Context, id string) (*docker.Network, error) {
	for _, n := range f.networks {
		if n.ID == id || n.Name == id {
			out := n
			return &out, nil
		}
	}
	return nil, fmt.Errorf("no such network: %s", id)
}

func (f *fakeClient) Volumes(context.Context) ([]docker.Volume, error) { return f.volumes, nil }

func (f *fakeClient) VolumeInspect(_ context.Context, name string) (*docker.Volume, error) {
	for _, v := range f.volumes {
		if v.Name == name {
			out := v
			return &out, nil
		}
	}
	return nil, fmt.Errorf("no such volume: %s", name)
}

// sampleClient returns a populated fake client.
func sampleClient() *fakeClient {
	return &fakeClient{
		containers: []docker.Container{
			{
				ID: "aaaaaaaaaaaa1111", Names: []string{"/web"}, Image: "nginx:1.25",
				ImageID: "sha256:img1", Command: "nginx -g 'daemon off;'",
				State: "running", Status: "Up 2 hours", Created: 1714550400,
				Ports: []docker.Port{{IP: "0.0.0.0", PublicPort: 8080, PrivatePort: 80, Type: "tcp"}},
			},
			{
				ID: "bbbbbbbbbbbb2222", Names: []string{"/db"}, Image: "postgres:16",
				ImageID: "sha256:img2", Command: "postgres",
				State: "exited", Status: "Exited (0) 5 minutes ago", Created: 1714460400,
			},
		},
		images: []docker.Image{
			{ID: "sha256:img1", RepoTags: []string{"nginx:1.25"}, Size: 143100000, Created: 1712000000},
			{ID: "sha256:img2", RepoTags: []string{"postgres:16"}, Size: 256000000, Created: 1711000000},
			{ID: "sha256:img3", RepoTags: nil, Size: 5000000, Created: 1710000000},
		},
		networks: []docker.Network{
			{Name: "bridge", ID: "net1", Driver: "bridge", Scope: "local",
				IPAM: &docker.IPAM{Driver: "default", Config: []docker.IPAMConfig{{Subnet: "172.17.0.0/16", Gateway: "172.17.0.1"}}}},
			{Name: "host", ID: "net2", Driver: "host", Scope: "local"},
		},
		volumes: []docker.Volume{
			{Name: "app-data", Driver: "local", Mountpoint: "/var/lib/docker/volumes/app-data/_data", Scope: "local"},
			{Name: "cache", Driver: "local", Mountpoint: "/var/lib/docker/volumes/cache/_data", Scope: "local"},
		},
		logs: map[string]string{
			"aaaaaaaaaaaa1111": "line one\nline two\nline three\n",
		},
	}
}

// newTestModel returns a loaded model sized like a normal terminal.
func newTestModel(t *testing.T, c docker.Client) Model {
	t.Helper()
	m := New(c)
	m = applySize(m, 120, 30)
	return loadData(t, m)
}

// selectContainer moves the cursor onto the named container. Lists are sorted
// by name by default, so tests must not assume the API's ordering.
func selectContainer(t *testing.T, m Model, name string) Model {
	t.Helper()
	m = press(t, m, "home")
	for i := 0; i < m.filteredCount(); i++ {
		if m.filteredContainers()[m.listConst().cursor].Name() == name {
			return m
		}
		m = press(t, m, "down")
	}
	t.Fatalf("container %q not found in the list", name)
	return m
}

// selectImage moves the cursor onto the image whose repository matches repo.
func selectImage(t *testing.T, m Model, repo string) Model {
	t.Helper()
	m = press(t, m, "home")
	for i := 0; i < m.filteredCount(); i++ {
		r, _ := imageRepoTag(m.filteredImages()[m.listConst().cursor])
		if r == repo {
			return m
		}
		m = press(t, m, "down")
	}
	t.Fatalf("image %q not found in the list", repo)
	return m
}

// applySize sends a window size message.
func applySize(m Model, w, h int) Model {
	next, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	return next.(Model)
}

// loadData runs the refresh command synchronously and applies the result.
func loadData(t *testing.T, m Model) Model {
	t.Helper()
	m.loading++
	msg := m.refreshCmd()()
	next, _ := m.Update(msg)
	return next.(Model)
}

// press sends a key to the model, running any returned command synchronously so
// asynchronous results (details, logs) land before assertions run.
func press(t *testing.T, m Model, key string) Model {
	t.Helper()
	next, cmd := m.Update(keyMsg(key))
	out := next.(Model)
	return drain(t, out, cmd)
}

// drain executes cmd (and batched children) applying every resulting message.
func drain(t *testing.T, m Model, cmd tea.Cmd) Model {
	t.Helper()
	if cmd == nil {
		return m
	}
	msg := cmd()
	switch typed := msg.(type) {
	case nil:
		return m
	case spinnerMsg:
		return m
	case tea.BatchMsg:
		for _, sub := range typed {
			m = drain(t, m, sub)
		}
		return m
	default:
		next, follow := m.Update(msg)
		m = next.(Model)
		if follow != nil {
			// Only follow non-tick commands to keep tests bounded.
			if _, isTick := msg.(spinnerMsg); !isTick {
				m = drainOnce(t, m, follow)
			}
		}
		return m
	}
}

// drainOnce applies a single follow-up command without recursing on ticks.
func drainOnce(t *testing.T, m Model, cmd tea.Cmd) Model {
	t.Helper()
	if cmd == nil {
		return m
	}
	msg := cmd()
	if _, ok := msg.(spinnerMsg); ok {
		return m
	}
	if batch, ok := msg.(tea.BatchMsg); ok {
		for _, sub := range batch {
			m = drainOnce(t, m, sub)
		}
		return m
	}
	next, _ := m.Update(msg)
	return next.(Model)
}

// keyMsg builds a tea.KeyMsg for a key name.
func keyMsg(key string) tea.KeyMsg {
	switch key {
	case "enter":
		return tea.KeyMsg{Type: tea.KeyEnter}
	case "esc":
		return tea.KeyMsg{Type: tea.KeyEsc}
	case "tab":
		return tea.KeyMsg{Type: tea.KeyTab}
	case "shift+tab":
		return tea.KeyMsg{Type: tea.KeyShiftTab}
	case "up":
		return tea.KeyMsg{Type: tea.KeyUp}
	case "down":
		return tea.KeyMsg{Type: tea.KeyDown}
	case "left":
		return tea.KeyMsg{Type: tea.KeyLeft}
	case "right":
		return tea.KeyMsg{Type: tea.KeyRight}
	case "pgup":
		return tea.KeyMsg{Type: tea.KeyPgUp}
	case "pgdown":
		return tea.KeyMsg{Type: tea.KeyPgDown}
	case "home":
		return tea.KeyMsg{Type: tea.KeyHome}
	case "end":
		return tea.KeyMsg{Type: tea.KeyEnd}
	case "backspace":
		return tea.KeyMsg{Type: tea.KeyBackspace}
	case "space":
		return tea.KeyMsg{Type: tea.KeySpace, Runes: []rune{' '}}
	default:
		return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
	}
}

// visible strips ANSI styling from a rendered view so assertions match text.
func visible(s string) string {
	var b strings.Builder
	inEscape := false
	for i := 0; i < len(s); i++ {
		ch := s[i]
		switch {
		case ch == 0x1b:
			inEscape = true
		case inEscape:
			if (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z') {
				inEscape = false
			}
		default:
			b.WriteByte(ch)
		}
	}
	return b.String()
}

// requireContains fails the test when the rendered view lacks a substring.
func requireContains(t *testing.T, screen string, wants ...string) {
	t.Helper()
	plain := visible(screen)
	for _, w := range wants {
		if !strings.Contains(plain, w) {
			t.Errorf("screen does not contain %q\n--- screen ---\n%s", w, plain)
		}
	}
}

func TestContainerListShowsNameAndImageTogether(t *testing.T) {
	m := newTestModel(t, sampleClient())
	screen := m.View()

	// Both containers, with names and images, must be on the same screen.
	requireContains(t, screen,
		"NAME", "IMAGE", "STATE", "STATUS", "PORTS", "CONTAINER ID",
		"web", "nginx:1.25", "running", "Up 2 hours", "0.0.0.0:8080->80/tcp",
		"db", "postgres:16", "exited",
	)
}

func TestImageListShowsSizeWithOneDecimal(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "2") // Images view

	screen := visible(m.View())
	for _, want := range []string{"REPOSITORY", "TAG", "SIZE", "IMAGE ID",
		"nginx", "1.25", "143.1 MB", "postgres", "16", "256.0 MB", "5.0 MB", "<none>"} {
		if !strings.Contains(screen, want) {
			t.Errorf("images screen missing %q\n%s", want, screen)
		}
	}
}

func TestNetworkListShowsNameAndDriver(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "3")
	requireContains(t, m.View(), "NAME", "DRIVER", "bridge", "host", "172.17.0.0/16")
}

func TestVolumeListShowsNameDriverAndMountpoint(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "4")
	requireContains(t, m.View(), "NAME", "DRIVER", "MOUNTPOINT",
		"app-data", "local", "/var/lib/docker/volumes/app-data/_data",
		"cache", "/var/lib/docker/volumes/cache/_data")
}

func TestVolumeDetailsShowsNameDriverMountpointOnOneScreen(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "4")
	m = press(t, m, "enter")

	if m.mode != modeDetail {
		t.Fatalf("mode = %v, want detail", m.mode)
	}
	requireContains(t, m.View(), "app-data", "Driver", "local",
		"Mountpoint", "/var/lib/docker/volumes/app-data/_data")
}

func TestContainerDetailsShowsRequiredFields(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = selectContainer(t, m, "web")
	m = press(t, m, "enter")

	if m.mode != modeDetail {
		t.Fatalf("mode = %v, want detail", m.mode)
	}
	// Name, image with tag, ports, command and ID must all be present.
	requireContains(t, m.View(),
		"web", "nginx:1.25", "Image tag", "1.25",
		"0.0.0.0:8080->80/tcp", "nginx -g daemon off;", "aaaaaaaaaaaa")
}

func TestImageDetailsShowsSameFields(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "2")
	m = selectImage(t, m, "nginx")
	m = press(t, m, "enter")

	if m.mode != modeDetail {
		t.Fatalf("mode = %v, want detail", m.mode)
	}
	requireContains(t, m.View(), "nginx", "Tag", "1.25", "143.1 MB", "img1")
}

func TestContainerLogsAreScrollable(t *testing.T) {
	c := sampleClient()
	// Enough lines to require scrolling in a short terminal.
	var b strings.Builder
	for i := 1; i <= 200; i++ {
		fmt.Fprintf(&b, "log line %d\n", i)
	}
	c.logs["aaaaaaaaaaaa1111"] = b.String()

	m := newTestModel(t, c)
	m = selectContainer(t, m, "web")
	m = press(t, m, "l")

	if m.mode != modeLogs {
		t.Fatalf("mode = %v, want logs", m.mode)
	}
	if len(m.logs.lines) != 200 {
		t.Fatalf("log lines = %d, want 200", len(m.logs.lines))
	}
	// The viewer opens at the tail, like `docker logs`.
	requireContains(t, m.View(), "log line 200")

	m = press(t, m, "home")
	requireContains(t, m.View(), "log line 1")
	if m.logs.offset != 0 {
		t.Errorf("offset after home = %d, want 0", m.logs.offset)
	}

	m = press(t, m, "pgdown")
	if m.logs.offset == 0 {
		t.Error("pgdown did not scroll the log viewer")
	}
	first := m.logs.offset
	m = press(t, m, "down")
	if m.logs.offset != first+1 {
		t.Errorf("offset after down = %d, want %d", m.logs.offset, first+1)
	}
	m = press(t, m, "end")
	if m.logs.offset != m.maxLogOffset() {
		t.Errorf("offset after end = %d, want %d", m.logs.offset, m.maxLogOffset())
	}
	// Scrolling must never run past the data.
	for i := 0; i < 50; i++ {
		m = press(t, m, "down")
	}
	if m.logs.offset != m.maxLogOffset() {
		t.Errorf("offset overran to %d, max is %d", m.logs.offset, m.maxLogOffset())
	}

	m = press(t, m, "esc")
	if m.mode != modeList {
		t.Errorf("esc did not return to the list, mode = %v", m.mode)
	}
}

func TestEmptyLogsReportedNotCrashing(t *testing.T) {
	c := sampleClient()
	m := newTestModel(t, c)
	m = selectContainer(t, m, "db") // db has no log output
	m = press(t, m, "l")

	if m.mode != modeLogs {
		t.Fatalf("mode = %v, want logs", m.mode)
	}
	requireContains(t, m.View(), "no log output")
}

func TestViewSwitchingWithTabAndLetters(t *testing.T) {
	m := newTestModel(t, sampleClient())
	if m.view != viewContainers {
		t.Fatalf("initial view = %v", m.view)
	}
	m = press(t, m, "tab")
	if m.view != viewImages {
		t.Errorf("tab -> %v, want images", m.view)
	}
	m = press(t, m, "tab")
	m = press(t, m, "tab")
	if m.view != viewVolumes {
		t.Errorf("three tabs -> %v, want volumes", m.view)
	}
	m = press(t, m, "tab") // wraps around
	if m.view != viewContainers {
		t.Errorf("tab wrap -> %v, want containers", m.view)
	}
	m = press(t, m, "shift+tab")
	if m.view != viewVolumes {
		t.Errorf("shift+tab -> %v, want volumes", m.view)
	}
	for key, want := range map[string]view{
		"c": viewContainers, "i": viewImages, "n": viewNetworks, "v": viewVolumes,
		"1": viewContainers, "2": viewImages, "3": viewNetworks, "4": viewVolumes,
	} {
		m = press(t, m, key)
		if m.view != want {
			t.Errorf("key %q -> %v, want %v", key, m.view, want)
		}
	}
}

func TestNavigationStaysInBounds(t *testing.T) {
	m := newTestModel(t, sampleClient())
	for i := 0; i < 20; i++ {
		m = press(t, m, "up")
	}
	if m.listConst().cursor != 0 {
		t.Errorf("cursor = %d after many ups, want 0", m.listConst().cursor)
	}
	for i := 0; i < 20; i++ {
		m = press(t, m, "down")
	}
	if want := m.filteredCount() - 1; m.listConst().cursor != want {
		t.Errorf("cursor = %d after many downs, want %d", m.listConst().cursor, want)
	}
	m = press(t, m, "home")
	if m.listConst().cursor != 0 {
		t.Errorf("home -> %d", m.listConst().cursor)
	}
	m = press(t, m, "end")
	if want := m.filteredCount() - 1; m.listConst().cursor != want {
		t.Errorf("end -> %d, want %d", m.listConst().cursor, want)
	}
}

func TestFilteringNarrowsTheList(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "/")
	if !m.listConst().filtering {
		t.Fatal("filter mode not active after /")
	}
	for _, r := range "postgres" {
		m = press(t, m, string(r))
	}
	if got := m.filteredCount(); got != 1 {
		t.Fatalf("filtered count = %d, want 1", got)
	}
	m = press(t, m, "enter")
	if m.listConst().filtering {
		t.Error("enter did not leave the filter prompt")
	}
	requireContains(t, m.View(), "postgres:16")
	if strings.Contains(visible(m.View()), "nginx:1.25") {
		t.Error("filtered-out container is still listed")
	}

	m = press(t, m, "esc")
	if m.listConst().filter != "" {
		t.Errorf("filter = %q after esc, want empty", m.listConst().filter)
	}
	if m.filteredCount() != 2 {
		t.Errorf("count after clearing filter = %d, want 2", m.filteredCount())
	}
}

func TestFilterBackspaceAndMultipleTerms(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "/")
	for _, r := range "nginxx" {
		m = press(t, m, string(r))
	}
	if m.filteredCount() != 0 {
		t.Fatalf("count = %d for a non-matching filter, want 0", m.filteredCount())
	}
	m = press(t, m, "backspace")
	if m.filteredCount() != 1 {
		t.Fatalf("count = %d after backspace, want 1", m.filteredCount())
	}
	// A second space separated term narrows further and must still match.
	m = press(t, m, "space")
	for _, r := range "running" {
		m = press(t, m, string(r))
	}
	if m.filteredCount() != 1 {
		t.Errorf("count = %d for \"nginx running\", want 1", m.filteredCount())
	}
}

func TestSortCyclingAndReversing(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "2") // images

	firstRepo := func(mm Model) string {
		imgs := mm.filteredImages()
		if len(imgs) == 0 {
			return ""
		}
		r, _ := imageRepoTag(imgs[0])
		return r
	}
	before := firstRepo(m)
	m = press(t, m, "S") // reverse
	if after := firstRepo(m); after == before {
		t.Errorf("reversing sort did not change the order (still %q)", after)
	}
	// Cycling through every column must stay in range and keep the list valid.
	for i := 0; i < len(sortOptions[viewImages])+1; i++ {
		m = press(t, m, "s")
		if m.listConst().sortIdx >= len(sortOptions[viewImages]) {
			t.Fatalf("sortIdx = %d out of range", m.listConst().sortIdx)
		}
		if len(m.filteredImages()) != 3 {
			t.Fatalf("sorting lost rows: %d", len(m.filteredImages()))
		}
	}
}

func TestSortBySizeOrdersImages(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "2")
	// Sort options for images are repository, tag, size, created.
	m = press(t, m, "s")
	m = press(t, m, "s")
	if got := sortOptions[viewImages][m.listConst().sortIdx].name; got != "size" {
		t.Fatalf("sort column = %q, want size", got)
	}
	imgs := m.filteredImages()
	if len(imgs) != 3 {
		t.Fatalf("images = %d", len(imgs))
	}
	if imgs[0].SizeBytes() < imgs[1].SizeBytes() || imgs[1].SizeBytes() < imgs[2].SizeBytes() {
		t.Errorf("images not sorted by descending size: %d, %d, %d",
			imgs[0].SizeBytes(), imgs[1].SizeBytes(), imgs[2].SizeBytes())
	}
}

func TestHelpScreenListsBindings(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "?")
	if m.mode != modeHelp {
		t.Fatalf("mode = %v, want help", m.mode)
	}
	// Scroll through the whole reference and collect it, since the help is
	// taller than the test terminal.
	var all strings.Builder
	all.WriteString(visible(m.View()))
	for i := 0; i < 20; i++ {
		m = press(t, m, "pgdown")
		all.WriteString(visible(m.View()))
	}
	text := all.String()
	for _, want := range []string{"keyboard reference", "quit", "filter", "logs", "details", "sort", "reload", "wrap"} {
		if !strings.Contains(text, want) {
			t.Errorf("help does not document %q", want)
		}
	}
	m = press(t, m, "esc")
	if m.mode != modeList {
		t.Errorf("esc did not close the help, mode = %v", m.mode)
	}
}

func TestStatusBarAlwaysShowsKeyHints(t *testing.T) {
	m := newTestModel(t, sampleClient())
	// Every mode must advertise how to get help and how to leave.
	checks := []struct {
		keys []string
		want []string
	}{
		{nil, []string{"?", "help", "q", "quit"}},
		{[]string{"enter"}, []string{"esc", "back"}},
		{[]string{"l"}, []string{"esc", "back", "wrap"}},
		{[]string{"?"}, []string{"close"}},
		{[]string{"/"}, []string{"filter", "apply"}},
	}
	for _, c := range checks {
		mm := m
		for _, k := range c.keys {
			mm = press(t, mm, k)
		}
		hints := visible(mm.renderKeyHints())
		for _, w := range c.want {
			if !strings.Contains(hints, w) {
				t.Errorf("after %v the hints %q do not mention %q", c.keys, hints, w)
			}
		}
	}
}

func TestSelectedRowIsMarked(t *testing.T) {
	m := newTestModel(t, sampleClient())
	screen := visible(m.View())
	if !strings.Contains(screen, "▸") {
		t.Errorf("selected row has no visual marker:\n%s", screen)
	}
	// Moving the cursor keeps exactly one marker.
	m = press(t, m, "down")
	if n := strings.Count(visible(m.View()), "▸"); n != 1 {
		t.Errorf("marker count = %d, want 1", n)
	}
}

func TestListErrorIsSurfaced(t *testing.T) {
	c := sampleClient()
	c.failContainers = true
	m := newTestModel(t, c)

	requireContains(t, m.View(), "connection refused")
	if m.statusLevel != levelError {
		t.Errorf("status level = %v, want error", m.statusLevel)
	}
	// The other views still work.
	m = press(t, m, "2")
	requireContains(t, m.View(), "143.1 MB")
}

func TestInspectErrorFallsBackToListFields(t *testing.T) {
	c := sampleClient()
	c.inspectErr = true
	m := newTestModel(t, c)
	m = selectContainer(t, m, "web")
	m = press(t, m, "enter")

	if m.mode != modeDetail {
		t.Fatalf("mode = %v, want detail", m.mode)
	}
	// The error is reported, but the fields already known are still shown.
	requireContains(t, m.View(), "no such container", "web", "nginx:1.25", "0.0.0.0:8080->80/tcp")
}

func TestDetailScrollingStaysInBounds(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = applySize(m, 100, 12) // short terminal forces scrolling
	m = selectContainer(t, m, "web")
	m = press(t, m, "enter")

	for i := 0; i < 100; i++ {
		m = press(t, m, "down")
	}
	max := maxOffset(len(m.detailLines()), m.detailPageSize())
	if m.detail.offset != max {
		t.Errorf("detail offset = %d, want %d", m.detail.offset, max)
	}
	for i := 0; i < 100; i++ {
		m = press(t, m, "up")
	}
	if m.detail.offset != 0 {
		t.Errorf("detail offset = %d after scrolling up, want 0", m.detail.offset)
	}
}

func TestLogsFromDetailPane(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = selectContainer(t, m, "web")
	m = press(t, m, "enter")
	m = press(t, m, "l")
	if m.mode != modeLogs {
		t.Fatalf("mode = %v, want logs", m.mode)
	}
	requireContains(t, m.View(), "line one", "line three")
}

func TestLogsRefusedOutsideContainers(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "3") // networks
	m = press(t, m, "l")
	if m.mode != modeList {
		t.Errorf("mode = %v, want list", m.mode)
	}
	if m.statusLevel != levelWarn {
		t.Errorf("status level = %v, want warn", m.statusLevel)
	}
}

func TestLogWrapToggle(t *testing.T) {
	c := sampleClient()
	c.logs["aaaaaaaaaaaa1111"] = strings.Repeat("x", 500) + "\n"
	m := newTestModel(t, c)
	m = selectContainer(t, m, "web")
	m = press(t, m, "l")

	if got := len(m.renderedLogLines()); got != 1 {
		t.Fatalf("unwrapped lines = %d, want 1", got)
	}
	m = press(t, m, "w")
	if !m.logs.wrap {
		t.Fatal("w did not enable wrapping")
	}
	if got := len(m.renderedLogLines()); got <= 1 {
		t.Errorf("wrapped lines = %d, want more than 1", got)
	}
	// Horizontal scrolling applies only when wrapping is off.
	m = press(t, m, "w")
	m = press(t, m, "right")
	if m.logs.xOffset == 0 {
		t.Error("right did not scroll sideways")
	}
	m = press(t, m, "w")
	if m.logs.xOffset != 0 {
		t.Error("enabling wrap did not reset the horizontal offset")
	}
}

func TestEmptyDataSetShowsGuidance(t *testing.T) {
	m := newTestModel(t, &fakeClient{logs: map[string]string{}})
	requireContains(t, m.View(), "No container", "Press r to reload")

	m = press(t, m, "enter") // nothing to inspect
	if m.mode != modeList {
		t.Errorf("mode = %v, want list", m.mode)
	}
	if m.statusLevel != levelWarn {
		t.Errorf("status = %v, want warn", m.statusLevel)
	}
}

func TestNoMatchGuidance(t *testing.T) {
	m := newTestModel(t, sampleClient())
	m = press(t, m, "/")
	for _, r := range "zzz" {
		m = press(t, m, string(r))
	}
	m = press(t, m, "enter")
	requireContains(t, m.View(), "No container matches", "esc to clear")
}

func TestReloadRefetches(t *testing.T) {
	c := sampleClient()
	m := newTestModel(t, c)
	c.containers = append(c.containers, docker.Container{
		ID: "cccccccccccc3333", Names: []string{"/cache"}, Image: "redis:7", State: "running", Status: "Up 1 minute",
	})
	m = press(t, m, "r")
	if got := len(m.data.containers); got != 3 {
		t.Fatalf("containers after reload = %d, want 3", got)
	}
	requireContains(t, m.View(), "cache", "redis:7")
}

func TestRenderedViewFitsTerminalBounds(t *testing.T) {
	sizes := [][2]int{{80, 24}, {120, 40}, {200, 60}, {60, 10}, {40, 8}}
	for _, s := range sizes {
		w, h := s[0], s[1]
		m := newTestModel(t, sampleClient())
		m = applySize(m, w, h)

		for _, keys := range [][]string{
			{}, {"enter"}, {"l"}, {"?"}, {"2"}, {"2", "enter"}, {"3"}, {"4"}, {"4", "enter"}, {"/"},
		} {
			mm := m
			for _, k := range keys {
				mm = press(t, mm, k)
			}
			lines := strings.Split(mm.View(), "\n")
			if len(lines) > h {
				t.Errorf("%dx%d keys %v: %d lines exceed height", w, h, keys, len(lines))
			}
			for i, l := range lines {
				if got := lipglossWidth(l); got > w {
					t.Errorf("%dx%d keys %v: line %d is %d cells wide, max %d\n%q",
						w, h, keys, i, got, w, visible(l))
				}
			}
		}
	}
}

func TestQuitKeys(t *testing.T) {
	m := newTestModel(t, sampleClient())
	_, cmd := m.Update(keyMsg("q"))
	if cmd == nil {
		t.Fatal("q produced no command")
	}
	if msg := cmd(); msg == nil {
		t.Error("q did not produce a quit message")
	}

	// q inside a sub-view backs out instead of quitting.
	m2 := press(t, m, "enter")
	m2 = press(t, m2, "q")
	if m2.mode != modeList {
		t.Errorf("q in detail mode did not return to the list, mode = %v", m2.mode)
	}

	_, cmd = m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
	if cmd == nil {
		t.Fatal("ctrl+c produced no command")
	}
}

// lipglossWidth measures the visible width of a rendered line.
func lipglossWidth(s string) int { return lipgloss.Width(s) }
