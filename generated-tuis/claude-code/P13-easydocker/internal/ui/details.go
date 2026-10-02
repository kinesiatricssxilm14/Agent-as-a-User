package ui

import (
	"fmt"
	"sort"
	"strings"
	"time"

	"toolm/internal/docker"
)

// fieldRow is one line of a detail pane. A row with an empty key and value
// renders as a blank separator; a row marked as a section renders as a heading.
type fieldRow struct {
	key     string
	value   string
	section bool
}

// field builds a key/value row.
func field(key, value string) fieldRow { return fieldRow{key: key, value: value} }

// section builds a heading row.
func section(title string) fieldRow { return fieldRow{key: title, section: true} }

// blank builds an empty spacer row.
func blank() fieldRow { return fieldRow{} }

// dash renders "-" for empty values so every field stays visible in the pane.
func dash(s string) string {
	if strings.TrimSpace(s) == "" {
		return "-"
	}
	return s
}

// formatUnix renders a Unix timestamp in local time, with a relative suffix.
func formatUnix(sec int64) string {
	if sec <= 0 {
		return "-"
	}
	t := time.Unix(sec, 0)
	return fmt.Sprintf("%s (%s)", t.Format("2006-01-02 15:04:05 MST"), humanizeAge(time.Since(t)))
}

// formatRFC3339 renders an API timestamp string, keeping the original when it
// cannot be parsed so nothing is silently dropped.
func formatRFC3339(s string) string {
	s = strings.TrimSpace(s)
	if s == "" || strings.HasPrefix(s, "0001-01-01") {
		return "-"
	}
	for _, layout := range []string{time.RFC3339Nano, time.RFC3339, "2006-01-02 15:04:05 -0700 MST", "2006-01-02T15:04:05.999999999Z"} {
		if t, err := time.Parse(layout, s); err == nil {
			return fmt.Sprintf("%s (%s)", t.Local().Format("2006-01-02 15:04:05 MST"), humanizeAge(time.Since(t)))
		}
	}
	return s
}

// humanizeAge renders a duration as a short "3 days ago" style string.
func humanizeAge(d time.Duration) string {
	if d < 0 {
		d = -d
	}
	switch {
	case d < time.Minute:
		return "just now"
	case d < time.Hour:
		return fmt.Sprintf("%d minutes ago", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%d hours ago", int(d.Hours()))
	case d < 30*24*time.Hour:
		return fmt.Sprintf("%d days ago", int(d.Hours()/24))
	case d < 365*24*time.Hour:
		return fmt.Sprintf("%d months ago", int(d.Hours()/24/30))
	default:
		return fmt.Sprintf("%d years ago", int(d.Hours()/24/365))
	}
}

// sortedKeys returns the map keys in deterministic order.
func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// containerRows builds the container detail pane from an inspect response,
// using the list entry to fill fields the inspect payload may omit.
func containerRows(d *docker.ContainerDetails, summary docker.Container) []fieldRow {
	if d == nil {
		return containerRowsFromSummary(summary)
	}

	name := nonEmpty(d.CleanName(), summary.Name())
	image := ""
	if d.Config != nil {
		image = d.Config.Image
	}
	image = nonEmpty(image, summary.Image)
	repo, tag := docker.SplitRepoTag(image)

	state := "-"
	status := ""
	if d.State != nil {
		state = nonEmpty(d.State.Status, summary.StateLabel())
		if d.State.Running {
			status = "Up"
			if d.State.StartedAt != "" {
				status = "Up since " + formatRFC3339(d.State.StartedAt)
			}
		} else if d.State.FinishedAt != "" && !strings.HasPrefix(d.State.FinishedAt, "0001-01-01") {
			status = fmt.Sprintf("Exited (%d) at %s", d.State.ExitCode, formatRFC3339(d.State.FinishedAt))
		}
	} else {
		state = summary.StateLabel()
	}
	status = nonEmpty(status, summary.Status)

	ports := d.PortMappings()
	if len(ports) == 0 && summary.PortsString() != "" {
		ports = strings.Split(summary.PortsString(), ", ")
	}

	command := nonEmpty(d.CommandLine(), summary.Command)

	// The fields the container view is about — name, image with tag, ports,
	// start command and ID — are placed first so they are all visible on one
	// screen even in a short terminal. Verbose sections follow below.
	rows := []fieldRow{
		section("Identity"),
		field("Name", dash(name)),
		field("Container ID", dash(docker.ShortID(nonEmpty(d.ID, summary.ID)))),
		field("Image", dash(image)),
		field("Image repository", dash(repo)),
		field("Image tag", dash(tag)),
		field("Image ID", dash(docker.ShortID(nonEmpty(d.Image, summary.ImageID)))),
		blank(),
		section("Command"),
		field("Command", dash(command)),
		blank(),
		section("Port mappings"),
	}
	if len(ports) == 0 {
		rows = append(rows, field("Ports", "none"))
	} else {
		for i, p := range ports {
			key := "Ports"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, p))
		}
	}

	rows = append(rows,
		blank(),
		section("State"),
		field("State", dash(state)),
		field("Status", dash(status)),
	)
	if d.State != nil {
		rows = append(rows,
			field("Exit code", fmt.Sprintf("%d", d.State.ExitCode)),
			field("PID", fmt.Sprintf("%d", d.State.Pid)),
			field("Started at", formatRFC3339(d.State.StartedAt)),
			field("Finished at", formatRFC3339(d.State.FinishedAt)),
		)
		if d.State.Error != "" {
			rows = append(rows, field("Error", d.State.Error))
		}
	}
	created := formatRFC3339(d.Created)
	if created == "-" && summary.Created > 0 {
		created = formatUnix(summary.Created)
	}
	rows = append(rows,
		field("Created", created),
		field("Restart count", fmt.Sprintf("%d", d.RestartCount)),
		blank(),
		section("Command details"),
		field("Full ID", dash(nonEmpty(d.ID, summary.ID))),
	)
	if d.Path != "" {
		rows = append(rows, field("Entrypoint path", d.Path))
	}
	if len(d.Args) > 0 {
		rows = append(rows, field("Arguments", strings.Join(d.Args, " ")))
	}
	if d.Config != nil {
		if len(d.Config.Entrypoint) > 0 {
			rows = append(rows, field("Config entrypoint", d.Config.Entrypoint.String()))
		}
		if len(d.Config.Cmd) > 0 {
			rows = append(rows, field("Config cmd", d.Config.Cmd.String()))
		}
		if d.Config.WorkingDir != "" {
			rows = append(rows, field("Working dir", d.Config.WorkingDir))
		}
		if d.Config.User != "" {
			rows = append(rows, field("User", d.Config.User))
		}
	}

	rows = append(rows, blank(), section("Networks"))
	if d.HostConfig != nil && d.HostConfig.NetworkMode != "" {
		rows = append(rows, field("Network mode", d.HostConfig.NetworkMode))
	}
	if d.NetworkSettings != nil && len(d.NetworkSettings.Networks) > 0 {
		for _, name := range sortedKeys(d.NetworkSettings.Networks) {
			ep := d.NetworkSettings.Networks[name]
			desc := name
			if ep != nil && ep.IPAddress != "" {
				desc = fmt.Sprintf("%s (ip %s", name, ep.IPAddress)
				if ep.IPPrefixLen > 0 {
					desc += fmt.Sprintf("/%d", ep.IPPrefixLen)
				}
				if ep.Gateway != "" {
					desc += ", gateway " + ep.Gateway
				}
				desc += ")"
			}
			rows = append(rows, field("Attached", desc))
		}
	} else if d.NetworkSettings != nil && d.NetworkSettings.IPAddress != "" {
		rows = append(rows, field("IP address", d.NetworkSettings.IPAddress))
	} else {
		rows = append(rows, field("Attached", "none"))
	}

	mounts := d.Mounts
	if len(mounts) == 0 {
		mounts = summary.Mounts
	}
	if len(mounts) > 0 {
		rows = append(rows, blank(), section("Mounts"))
		for _, mt := range mounts {
			src := nonEmpty(mt.Name, mt.Source)
			mode := "ro"
			if mt.RW {
				mode = "rw"
			}
			rows = append(rows, field(dash(mt.Type), fmt.Sprintf("%s -> %s (%s)", dash(src), dash(mt.Destination), mode)))
		}
	}

	if d.Config != nil && len(d.Config.Env) > 0 {
		rows = append(rows, blank(), section("Environment"))
		for _, e := range d.Config.Env {
			k, v, found := strings.Cut(e, "=")
			if !found {
				rows = append(rows, field(e, ""))
				continue
			}
			rows = append(rows, field(k, v))
		}
	}

	labels := map[string]string{}
	if d.Config != nil {
		labels = d.Config.Labels
	}
	if len(labels) == 0 {
		labels = summary.Labels
	}
	if len(labels) > 0 {
		rows = append(rows, blank(), section("Labels"))
		for _, k := range sortedKeys(labels) {
			rows = append(rows, field(k, labels[k]))
		}
	}

	rows = append(rows, blank(), section("Runtime"))
	if d.Platform != "" {
		rows = append(rows, field("Platform", d.Platform))
	}
	if d.Driver != "" {
		rows = append(rows, field("Storage driver", d.Driver))
	}
	if d.LogPath != "" {
		rows = append(rows, field("Log path", d.LogPath))
	}
	if d.HostConfig != nil {
		rows = append(rows, field("Privileged", fmt.Sprintf("%t", d.HostConfig.Privileged)))
		if d.HostConfig.RestartPolicy != nil && d.HostConfig.RestartPolicy.Name != "" {
			rows = append(rows, field("Restart policy", d.HostConfig.RestartPolicy.Name))
		}
	}
	rows = append(rows, field("Press l", "view this container's logs"))
	return rows
}

// containerRowsFromSummary builds a reduced detail pane from the list entry.
// It is used when inspect is unavailable so the pane still shows real data.
func containerRowsFromSummary(c docker.Container) []fieldRow {
	repo, tag := docker.SplitRepoTag(c.Image)
	rows := []fieldRow{
		section("Identity (from container list)"),
		field("Name", dash(c.Name())),
		field("Container ID", dash(docker.ShortID(c.ID))),
		field("Image", dash(c.Image)),
		field("Image repository", dash(repo)),
		field("Image tag", dash(tag)),
		field("Image ID", dash(docker.ShortID(c.ImageID))),
		blank(),
		section("Command"),
		field("Command", dash(c.Command)),
		blank(),
		section("Port mappings"),
	}
	if len(c.Ports) == 0 {
		rows = append(rows, field("Ports", "none"))
	} else {
		for i, p := range c.Ports {
			key := "Ports"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, p.String()))
		}
	}
	rows = append(rows,
		blank(),
		section("State"),
		field("State", dash(c.StateLabel())),
		field("Status", dash(c.Status)),
		field("Created", formatUnix(c.Created)),
		field("Full ID", dash(c.ID)),
	)
	return rows
}

// imageRows builds the image detail pane.
func imageRows(d *docker.ImageDetails, summary docker.Image) []fieldRow {
	if d == nil {
		return imageRowsFromSummary(summary)
	}
	tags := d.RepoTags
	if len(tags) == 0 {
		tags = summary.RepoTags
	}
	repo, tag := "<none>", "<none>"
	if len(tags) > 0 {
		repo, tag = docker.SplitRepoTag(tags[0])
	} else if len(d.RepoDigests) > 0 {
		if i := strings.Index(d.RepoDigests[0], "@"); i > 0 {
			repo = d.RepoDigests[0][:i]
		}
	}

	size := d.SizeBytes()
	if size == 0 {
		size = summary.SizeBytes()
	}

	rows := []fieldRow{
		section("Identity"),
		field("Name", dash(repo)),
		field("Repository", dash(repo)),
		field("Tag", dash(tag)),
		field("Image ID", dash(docker.ShortID(nonEmpty(d.ID, summary.ID)))),
		field("Full ID", dash(nonEmpty(d.ID, summary.ID))),
		blank(),
		section("Size"),
		field("Size", docker.FormatSizeMB(size)),
		field("Size (binary)", docker.FormatSizeMiB(size)),
		field("Size (bytes)", fmt.Sprintf("%d", size)),
	}
	if summary.SharedSize > 0 {
		rows = append(rows, field("Shared size", docker.FormatSizeMB(summary.SharedSize)))
	}

	rows = append(rows, blank(), section("Repository tags"))
	if len(tags) == 0 {
		rows = append(rows, field("Tags", "<none>"))
	} else {
		for i, t := range tags {
			key := "Tags"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, t))
		}
	}
	if len(d.RepoDigests) > 0 {
		rows = append(rows, blank(), section("Digests"))
		for i, dg := range d.RepoDigests {
			key := "Digest"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, dg))
		}
	}

	created := formatRFC3339(d.Created)
	if created == "-" && summary.Created > 0 {
		created = formatUnix(summary.Created)
	}
	rows = append(rows,
		blank(),
		section("Build"),
		field("Created", created),
		field("Architecture", dash(d.Architecture)),
		field("OS", dash(d.Os)),
		field("Docker version", dash(d.DockerVersion)),
		field("Author", dash(d.Author)),
		field("Parent", dash(docker.ShortID(d.Parent))),
	)
	if d.Comment != "" {
		rows = append(rows, field("Comment", d.Comment))
	}

	if d.Config != nil {
		rows = append(rows, blank(), section("Configuration"))
		rows = append(rows,
			field("Entrypoint", dash(d.Config.Entrypoint.String())),
			field("Cmd", dash(d.Config.Cmd.String())),
			field("Working dir", dash(d.Config.WorkingDir)),
			field("User", dash(d.Config.User)),
		)
		if len(d.Config.ExposedPorts) > 0 {
			for i, p := range sortedKeys(d.Config.ExposedPorts) {
				key := "Exposed ports"
				if i > 0 {
					key = ""
				}
				rows = append(rows, field(key, p))
			}
		}
		if len(d.Config.Env) > 0 {
			rows = append(rows, blank(), section("Environment"))
			for _, e := range d.Config.Env {
				k, v, found := strings.Cut(e, "=")
				if !found {
					rows = append(rows, field(e, ""))
					continue
				}
				rows = append(rows, field(k, v))
			}
		}
		if len(d.Config.Labels) > 0 {
			rows = append(rows, blank(), section("Labels"))
			for _, k := range sortedKeys(d.Config.Labels) {
				rows = append(rows, field(k, d.Config.Labels[k]))
			}
		}
	}

	if d.RootFS != nil && len(d.RootFS.Layers) > 0 {
		rows = append(rows, blank(), section("Layers"))
		for i, l := range d.RootFS.Layers {
			rows = append(rows, field(fmt.Sprintf("Layer %d", i+1), l))
		}
	}
	return rows
}

// imageRowsFromSummary builds an image detail pane from the list entry.
func imageRowsFromSummary(img docker.Image) []fieldRow {
	repo, tag := imageRepoTag(img)
	rows := []fieldRow{
		section("Identity (from image list)"),
		field("Name", dash(repo)),
		field("Repository", dash(repo)),
		field("Tag", dash(tag)),
		field("Image ID", dash(docker.ShortID(img.ID))),
		field("Full ID", dash(img.ID)),
		blank(),
		section("Size"),
		field("Size", docker.FormatSizeMB(img.SizeBytes())),
		field("Size (binary)", docker.FormatSizeMiB(img.SizeBytes())),
		field("Size (bytes)", fmt.Sprintf("%d", img.SizeBytes())),
		blank(),
		section("Build"),
		field("Created", formatUnix(img.Created)),
	}
	if len(img.RepoTags) > 0 {
		rows = append(rows, blank(), section("Repository tags"))
		for i, t := range img.RepoTags {
			key := "Tags"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, t))
		}
	}
	return rows
}

// networkRows builds the network detail pane.
func networkRows(n *docker.Network) []fieldRow {
	if n == nil {
		return nil
	}
	rows := []fieldRow{
		section("Identity"),
		field("Name", dash(n.Name)),
		field("Driver", dash(n.DriverName())),
		field("Network ID", dash(docker.ShortID(n.ID))),
		field("Full ID", dash(n.ID)),
		field("Scope", dash(n.Scope)),
		field("Created", formatRFC3339(n.Created)),
		blank(),
		section("Flags"),
		field("Internal", fmt.Sprintf("%t", n.Internal)),
		field("Attachable", fmt.Sprintf("%t", n.Attachable)),
		field("Ingress", fmt.Sprintf("%t", n.Ingress)),
		field("IPv6 enabled", fmt.Sprintf("%t", n.EnableIPv6)),
		blank(),
		section("IPAM"),
	}
	if n.IPAM != nil {
		rows = append(rows, field("IPAM driver", dash(n.IPAM.Driver)))
	} else {
		rows = append(rows, field("IPAM driver", "-"))
	}
	subnets := n.Subnets()
	if len(subnets) == 0 {
		rows = append(rows, field("Subnet", "-"))
	} else {
		for i, s := range subnets {
			key := "Subnet"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, s))
		}
	}
	gws := n.Gateways()
	if len(gws) == 0 {
		rows = append(rows, field("Gateway", "-"))
	} else {
		for i, g := range gws {
			key := "Gateway"
			if i > 0 {
				key = ""
			}
			rows = append(rows, field(key, g))
		}
	}

	rows = append(rows, blank(), section("Connected containers"))
	if len(n.Containers) == 0 {
		rows = append(rows, field("Containers", "none"))
	} else {
		for _, id := range sortedKeys(n.Containers) {
			c := n.Containers[id]
			desc := dash(c.Name)
			if c.IPv4Address != "" {
				desc += " (" + c.IPv4Address + ")"
			}
			rows = append(rows, field(docker.ShortID(id), desc))
		}
	}
	if len(n.Options) > 0 {
		rows = append(rows, blank(), section("Options"))
		for _, k := range sortedKeys(n.Options) {
			rows = append(rows, field(k, n.Options[k]))
		}
	}
	if len(n.Labels) > 0 {
		rows = append(rows, blank(), section("Labels"))
		for _, k := range sortedKeys(n.Labels) {
			rows = append(rows, field(k, n.Labels[k]))
		}
	}
	return rows
}

// volumeRows builds the volume detail pane. Name, driver and mountpoint are
// listed first because they are the fields the volume view is about.
func volumeRows(v *docker.Volume) []fieldRow {
	if v == nil {
		return nil
	}
	rows := []fieldRow{
		section("Identity"),
		field("Name", dash(v.Name)),
		field("Driver", dash(v.DriverName())),
		field("Mountpoint", dash(v.Mountpoint)),
		field("Scope", dash(v.Scope)),
		field("Created", formatRFC3339(v.CreatedAt)),
	}
	if v.UsageData != nil {
		rows = append(rows,
			blank(),
			section("Usage"),
			field("Size", docker.FormatSizeMB(v.UsageData.Size)),
			field("Size (bytes)", fmt.Sprintf("%d", v.UsageData.Size)),
			field("Reference count", fmt.Sprintf("%d", v.UsageData.RefCount)),
		)
	}
	if len(v.Options) > 0 {
		rows = append(rows, blank(), section("Options"))
		for _, k := range sortedKeys(v.Options) {
			rows = append(rows, field(k, v.Options[k]))
		}
	}
	if len(v.Labels) > 0 {
		rows = append(rows, blank(), section("Labels"))
		for _, k := range sortedKeys(v.Labels) {
			rows = append(rows, field(k, v.Labels[k]))
		}
	}
	if len(v.Status) > 0 {
		rows = append(rows, blank(), section("Status"))
		for _, k := range sortedKeys(v.Status) {
			rows = append(rows, field(k, fmt.Sprintf("%v", v.Status[k])))
		}
	}
	return rows
}
