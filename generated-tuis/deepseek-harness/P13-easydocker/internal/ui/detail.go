package ui

import (
	"fmt"
	"strings"

	"toolm/internal/docker"
)

func buildContainerDetail(insp docker.ContainerInspect) (string, []string) {
	title := "Container: " + strings.TrimPrefix(insp.Name, "/")
	var lines []string
	lines = append(lines, kv("Name", strings.TrimPrefix(insp.Name, "/")))
	lines = append(lines, kv("ID", insp.ID))
	lines = append(lines, kv("Image", imageOf(insp)))
	lines = append(lines, kv("Created", formatTime(insp.Created)))
	lines = append(lines, kv("Command", commandString(insp)))
	if insp.State != nil {
		lines = append(lines, kv("Status", insp.State.Status))
		lines = append(lines, kv("Running", strBool(insp.State.Running)))
		lines = append(lines, kv("Paused", strBool(insp.State.Paused)))
		lines = append(lines, kv("Restarting", strBool(insp.State.Restarting)))
		lines = append(lines, kv("PID", fmt.Sprintf("%d", insp.State.Pid)))
		lines = append(lines, kv("Exit Code", fmt.Sprintf("%d", insp.State.ExitCode)))
		lines = append(lines, kv("Started", formatTime(insp.State.StartedAt)))
		lines = append(lines, kv("Finished", formatTime(insp.State.FinishedAt)))
	}
	addSection(&lines, "Ports", portLines(insp))
	addSection(&lines, "Mounts", mountLines(insp))
	if insp.NetworkSettings != nil {
		addSection(&lines, "Networks", networkLines(*insp.NetworkSettings))
	}
	if insp.Config != nil {
		lines = append(lines, kv("Working Dir", insp.Config.WorkingDir))
		addSection(&lines, "Environment", insp.Config.Env)
	}
	if insp.HostConfig != nil {
		lines = append(lines, kv("Network Mode", insp.HostConfig.NetworkMode))
		lines = append(lines, kv("Restart", insp.HostConfig.RestartPolicy.Name))
	}
	return title, lines
}

func buildImageDetail(img docker.ImageInspect) (string, []string) {
	repo, tag := firstRepoTag(img.RepoTags)
	title := "Image: " + repo + ":" + tag
	var lines []string
	lines = append(lines, kv("ID", img.ID))
	lines = append(lines, kv("Repository", repo))
	lines = append(lines, kv("Tag", tag))
	lines = append(lines, kv("Size", formatSizeMB(img.Size)))
	lines = append(lines, kv("Created", formatTime(img.Created)))
	lines = append(lines, kv("Architecture", img.Architecture))
	lines = append(lines, kv("OS", img.Os))
	addSection(&lines, "RepoTags", img.RepoTags)
	addSection(&lines, "Digests", img.RepoDigests)
	if img.Config != nil {
		lines = append(lines, kv("Command", shellJoin(append(img.Config.Entrypoint, img.Config.Cmd...))))
		lines = append(lines, kv("Working Dir", img.Config.WorkingDir))
		addSection(&lines, "Exposed Ports", exposedPorts(img.Config.ExposedPorts))
		addSection(&lines, "Environment", img.Config.Env)
	}
	return title, lines
}

func buildNetworkDetail(n docker.NetworkInspect) (string, []string) {
	title := "Network: " + n.Name
	var lines []string
	lines = append(lines, kv("ID", n.ID))
	lines = append(lines, kv("Name", n.Name))
	lines = append(lines, kv("Driver", n.Driver))
	lines = append(lines, kv("Scope", n.Scope))
	lines = append(lines, kv("Internal", strBool(n.Internal)))
	lines = append(lines, kv("Attachable", strBool(n.Attachable)))
	lines = append(lines, kv("Ingress", strBool(n.Ingress)))
	lines = append(lines, kv("IPAM Driver", n.IPAM.Driver))
	var subnets []string
	for _, cfg := range n.IPAM.Config {
		s := cfg.Subnet
		if cfg.Gateway != "" {
			s += " (gateway " + cfg.Gateway + ")"
		}
		subnets = append(subnets, s)
	}
	addSection(&lines, "Subnets", subnets)
	var attached []string
	for id, c := range n.Containers {
		attached = append(attached, fmt.Sprintf("%s %s %s", c.Name, shortID(id), c.IPv4Address))
	}
	addSection(&lines, "Containers", attached)
	addSection(&lines, "Options", mapLines(n.Options))
	return title, lines
}

func buildVolumeDetail(v docker.Volume) (string, []string) {
	title := "Volume: " + v.Name
	var lines []string
	lines = append(lines, kv("Name", v.Name))
	lines = append(lines, kv("Driver", v.Driver))
	lines = append(lines, kv("Mountpoint", v.Mountpoint))
	lines = append(lines, kv("Scope", v.Scope))
	lines = append(lines, kv("Created", formatTime(v.CreatedAt)))
	addSection(&lines, "Options", mapLines(v.Options))
	addSection(&lines, "Labels", mapLines(v.Labels))
	return title, lines
}
