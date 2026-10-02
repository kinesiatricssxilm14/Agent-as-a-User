package ui

import (
	"context"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"toolm/internal/docker"
)

const (
	requestTimeout = 30 * time.Second
	logTimeout     = 120 * time.Second
)

// ---- list load messages ----

type containersMsg struct {
	items []docker.Container
	err   error
}

type imagesMsg struct {
	items []docker.Image
	err   error
}

type networksMsg struct {
	items []docker.Network
	err   error
}

type volumesMsg struct {
	items []docker.Volume
	err   error
}

type detailMsg struct {
	tab   tabKind
	id    string
	name  string
	title string
	lines []string
	err   error
}

type logsMsg struct {
	id    string
	name  string
	lines []string
	err   error
}

type actionResultMsg struct {
	tab  tabKind
	desc string
	err  error
}

type messageTickMsg struct {
	gen int
}

func loadContainersCmd(c *docker.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		items, err := c.ListContainers(ctx)
		return containersMsg{items: items, err: err}
	}
}

func loadImagesCmd(c *docker.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		items, err := c.ListImages(ctx)
		return imagesMsg{items: items, err: err}
	}
}

func loadNetworksCmd(c *docker.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		items, err := c.ListNetworks(ctx)
		return networksMsg{items: items, err: err}
	}
}

func loadVolumesCmd(c *docker.Client) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		items, err := c.ListVolumes(ctx)
		return volumesMsg{items: items, err: err}
	}
}

func loadContainerDetailCmd(c *docker.Client, id, name string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		insp, err := c.InspectContainer(ctx, id)
		if err != nil {
			return detailMsg{tab: tabContainers, id: id, name: name, err: err}
		}
		title, lines := buildContainerDetail(*insp)
		return detailMsg{tab: tabContainers, id: id, name: name, title: title, lines: lines}
	}
}

func loadImageDetailCmd(c *docker.Client, id, name string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		insp, err := c.InspectImage(ctx, id)
		if err != nil {
			return detailMsg{tab: tabImages, id: id, name: name, err: err}
		}
		title, lines := buildImageDetail(*insp)
		return detailMsg{tab: tabImages, id: id, name: name, title: title, lines: lines}
	}
}

func loadNetworkDetailCmd(c *docker.Client, id, name string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		insp, err := c.InspectNetwork(ctx, id)
		if err != nil {
			return detailMsg{tab: tabNetworks, id: id, name: name, err: err}
		}
		title, lines := buildNetworkDetail(*insp)
		return detailMsg{tab: tabNetworks, id: id, name: name, title: title, lines: lines}
	}
}

func loadVolumeDetailCmd(c *docker.Client, name string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		insp, err := c.InspectVolume(ctx, name)
		if err != nil {
			return detailMsg{tab: tabVolumes, id: name, name: name, err: err}
		}
		title, lines := buildVolumeDetail(*insp)
		return detailMsg{tab: tabVolumes, id: name, name: name, title: title, lines: lines}
	}
}

func loadDetailCmd(c *docker.Client, tab tabKind, id, name string) tea.Cmd {
	switch tab {
	case tabContainers:
		return loadContainerDetailCmd(c, id, name)
	case tabImages:
		return loadImageDetailCmd(c, id, name)
	case tabNetworks:
		return loadNetworkDetailCmd(c, id, name)
	case tabVolumes:
		return loadVolumeDetailCmd(c, name)
	}
	return nil
}

func loadLogsCmd(c *docker.Client, id, name string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), logTimeout)
		defer cancel()
		text, err := c.ContainerLogs(ctx, id)
		if err != nil {
			return logsMsg{id: id, name: name, err: err}
		}
		return logsMsg{id: id, name: name, lines: splitLines(text)}
	}
}

func splitLines(s string) []string {
	if s == "" {
		return nil
	}
	s = strings.TrimRight(s, "\n")
	return strings.Split(s, "\n")
}

func runCmd(tab tabKind, desc string, fn func(context.Context) error) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		err := fn(ctx)
		return actionResultMsg{tab: tab, desc: desc, err: err}
	}
}
