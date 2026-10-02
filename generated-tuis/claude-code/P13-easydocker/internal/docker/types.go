package docker

import (
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"
)

// Port is an entry of the "Ports" array returned by /containers/json.
type Port struct {
	IP          string `json:"IP"`
	PrivatePort int    `json:"PrivatePort"`
	PublicPort  int    `json:"PublicPort"`
	Type        string `json:"Type"`
}

// String renders a port mapping the way the Docker CLI does, e.g.
// "0.0.0.0:8080->80/tcp" for a published port or "80/tcp" for one that is
// merely exposed.
func (p Port) String() string {
	proto := p.Type
	if proto == "" {
		proto = "tcp"
	}
	if p.PublicPort == 0 {
		return fmt.Sprintf("%d/%s", p.PrivatePort, proto)
	}
	host := p.IP
	if host == "" {
		host = "0.0.0.0"
	}
	return fmt.Sprintf("%s:%d->%d/%s", host, p.PublicPort, p.PrivatePort, proto)
}

// PortBinding is a host side binding as reported by NetworkSettings.Ports.
type PortBinding struct {
	HostIP   string `json:"HostIp"`
	HostPort string `json:"HostPort"`
}

// Mount describes a bind mount or volume attached to a container.
type Mount struct {
	Type        string `json:"Type"`
	Name        string `json:"Name"`
	Source      string `json:"Source"`
	Destination string `json:"Destination"`
	Driver      string `json:"Driver"`
	Mode        string `json:"Mode"`
	RW          bool   `json:"RW"`
	Propagation string `json:"Propagation"`
}

// EndpointSettings is a container's attachment to one network.
type EndpointSettings struct {
	NetworkID   string `json:"NetworkID"`
	EndpointID  string `json:"EndpointID"`
	Gateway     string `json:"Gateway"`
	IPAddress   string `json:"IPAddress"`
	IPPrefixLen int    `json:"IPPrefixLen"`
	MacAddress  string `json:"MacAddress"`
	Aliases     []string
}

// NetworkSettings holds the network related part of a container.
type NetworkSettings struct {
	IPAddress string                       `json:"IPAddress"`
	Gateway   string                       `json:"Gateway"`
	Ports     map[string][]PortBinding     `json:"Ports"`
	Networks  map[string]*EndpointSettings `json:"Networks"`
}

// Container is one entry of GET /containers/json?all=1.
type Container struct {
	ID              string            `json:"Id"`
	Names           []string          `json:"Names"`
	Image           string            `json:"Image"`
	ImageID         string            `json:"ImageID"`
	Command         string            `json:"Command"`
	Created         int64             `json:"Created"`
	State           string            `json:"State"`
	Status          string            `json:"Status"`
	Ports           []Port            `json:"Ports"`
	Labels          map[string]string `json:"Labels"`
	Mounts          []Mount           `json:"Mounts"`
	NetworkSettings *NetworkSettings  `json:"NetworkSettings"`
	SizeRw          int64             `json:"SizeRw"`
	SizeRootFs      int64             `json:"SizeRootFs"`
}

// Name returns the primary container name without the leading slash that the
// Docker API adds. Containers without a name fall back to their short ID.
func (c Container) Name() string {
	for _, n := range c.Names {
		n = strings.TrimPrefix(strings.TrimSpace(n), "/")
		if n != "" {
			return n
		}
	}
	return ShortID(c.ID)
}

// PortsString renders every port mapping of the container, comma separated.
func (c Container) PortsString() string {
	if len(c.Ports) == 0 {
		return ""
	}
	parts := make([]string, 0, len(c.Ports))
	for _, p := range c.Ports {
		parts = append(parts, p.String())
	}
	sort.Strings(parts)
	return strings.Join(parts, ", ")
}

// StateLabel is the coarse container state ("running", "exited", ...). Some
// API versions only fill Status, so it is used as a fallback.
func (c Container) StateLabel() string {
	if c.State != "" {
		return strings.ToLower(c.State)
	}
	if c.Status != "" {
		if strings.HasPrefix(c.Status, "Up") {
			return "running"
		}
		return strings.ToLower(strings.Fields(c.Status)[0])
	}
	return "unknown"
}

// IsRunning reports whether the container is currently up.
func (c Container) IsRunning() bool {
	switch c.StateLabel() {
	case "running", "restarting":
		return true
	}
	return strings.HasPrefix(c.Status, "Up")
}

// ContainerState is the State object of a container inspect response.
type ContainerState struct {
	Status     string `json:"Status"`
	Running    bool   `json:"Running"`
	Paused     bool   `json:"Paused"`
	Restarting bool   `json:"Restarting"`
	OOMKilled  bool   `json:"OOMKilled"`
	Dead       bool   `json:"Dead"`
	Pid        int    `json:"Pid"`
	ExitCode   int    `json:"ExitCode"`
	Error      string `json:"Error"`
	StartedAt  string `json:"StartedAt"`
	FinishedAt string `json:"FinishedAt"`
}

// Config is the image/container configuration block. It is shared by the
// container and image inspect responses.
type Config struct {
	Hostname     string              `json:"Hostname"`
	User         string              `json:"User"`
	Image        string              `json:"Image"`
	WorkingDir   string              `json:"WorkingDir"`
	Entrypoint   StringList          `json:"Entrypoint"`
	Cmd          StringList          `json:"Cmd"`
	Env          []string            `json:"Env"`
	Labels       map[string]string   `json:"Labels"`
	ExposedPorts map[string]struct{} `json:"ExposedPorts"`
	Tty          bool                `json:"Tty"`
	OpenStdin    bool                `json:"OpenStdin"`
}

// StringList accepts either a JSON array of strings or a single string, since
// Cmd and Entrypoint are reported both ways in the wild.
type StringList []string

// UnmarshalJSON implements json.Unmarshaler.
func (s *StringList) UnmarshalJSON(data []byte) error {
	trimmed := strings.TrimSpace(string(data))
	if trimmed == "" || trimmed == "null" {
		*s = nil
		return nil
	}
	if trimmed[0] == '[' {
		var list []string
		if err := json.Unmarshal(data, &list); err != nil {
			return err
		}
		*s = list
		return nil
	}
	var single string
	if err := json.Unmarshal(data, &single); err != nil {
		return err
	}
	if single == "" {
		*s = nil
		return nil
	}
	*s = StringList{single}
	return nil
}

// String joins the list into a single shell-ish command line.
func (s StringList) String() string { return strings.Join(s, " ") }

// HostConfig is the subset of a container's host configuration we display.
type HostConfig struct {
	NetworkMode   string                   `json:"NetworkMode"`
	PortBindings  map[string][]PortBinding `json:"PortBindings"`
	Privileged    bool                     `json:"Privileged"`
	AutoRemove    bool                     `json:"AutoRemove"`
	Binds         []string                 `json:"Binds"`
	RestartPolicy *struct {
		Name              string `json:"Name"`
		MaximumRetryCount int    `json:"MaximumRetryCount"`
	} `json:"RestartPolicy"`
}

// ContainerDetails is the response of GET /containers/{id}/json.
type ContainerDetails struct {
	ID              string           `json:"Id"`
	Name            string           `json:"Name"`
	Created         string           `json:"Created"`
	Path            string           `json:"Path"`
	Args            []string         `json:"Args"`
	Image           string           `json:"Image"`
	Driver          string           `json:"Driver"`
	Platform        string           `json:"Platform"`
	RestartCount    int              `json:"RestartCount"`
	LogPath         string           `json:"LogPath"`
	State           *ContainerState  `json:"State"`
	Config          *Config          `json:"Config"`
	HostConfig      *HostConfig      `json:"HostConfig"`
	NetworkSettings *NetworkSettings `json:"NetworkSettings"`
	Mounts          []Mount          `json:"Mounts"`
}

// CleanName returns the container name without the API's leading slash.
func (d *ContainerDetails) CleanName() string {
	if d == nil {
		return ""
	}
	return strings.TrimPrefix(d.Name, "/")
}

// CommandLine reconstructs the container start command from Path/Args, falling
// back to the image entrypoint and cmd.
func (d *ContainerDetails) CommandLine() string {
	if d == nil {
		return ""
	}
	if d.Path != "" {
		parts := append([]string{d.Path}, d.Args...)
		return strings.Join(parts, " ")
	}
	if d.Config != nil {
		parts := append([]string{}, d.Config.Entrypoint...)
		parts = append(parts, d.Config.Cmd...)
		return strings.Join(parts, " ")
	}
	return ""
}

// PortMappings renders the container's port bindings as CLI style strings. It
// prefers the host bindings and falls back to merely exposed ports.
func (d *ContainerDetails) PortMappings() []string {
	if d == nil {
		return nil
	}
	seen := map[string]bool{}
	var out []string
	add := func(s string) {
		if s == "" || seen[s] {
			return
		}
		seen[s] = true
		out = append(out, s)
	}

	bindings := map[string][]PortBinding{}
	if d.NetworkSettings != nil {
		for k, v := range d.NetworkSettings.Ports {
			bindings[k] = v
		}
	}
	if d.HostConfig != nil {
		for k, v := range d.HostConfig.PortBindings {
			if len(bindings[k]) == 0 {
				bindings[k] = v
			}
		}
	}
	// covered records container ports that already have a published mapping, so
	// the ExposedPorts pass below does not list them a second time.
	covered := map[string]bool{}
	for containerPort, hostPorts := range bindings {
		spec := normalizePortSpec(containerPort)
		if len(hostPorts) == 0 {
			add(spec)
			covered[spec] = true
			continue
		}
		for _, hp := range hostPorts {
			host := hp.HostIP
			if host == "" {
				host = "0.0.0.0"
			}
			if hp.HostPort == "" {
				add(spec)
				covered[spec] = true
				continue
			}
			add(fmt.Sprintf("%s:%s->%s", host, hp.HostPort, spec))
			covered[spec] = true
		}
	}
	if d.Config != nil {
		for containerPort := range d.Config.ExposedPorts {
			spec := normalizePortSpec(containerPort)
			if !covered[spec] {
				add(spec)
			}
		}
	}
	sort.Strings(out)
	return out
}

// normalizePortSpec makes sure a port spec carries a protocol suffix.
func normalizePortSpec(spec string) string {
	if spec == "" {
		return ""
	}
	if strings.Contains(spec, "/") {
		return spec
	}
	return spec + "/tcp"
}

// Image is one entry of GET /images/json.
type Image struct {
	ID          string            `json:"Id"`
	ParentID    string            `json:"ParentId"`
	RepoTags    []string          `json:"RepoTags"`
	RepoDigests []string          `json:"RepoDigests"`
	Created     int64             `json:"Created"`
	Size        int64             `json:"Size"`
	VirtualSize int64             `json:"VirtualSize"`
	SharedSize  int64             `json:"SharedSize"`
	Containers  int64             `json:"Containers"`
	Labels      map[string]string `json:"Labels"`
}

// SizeBytes returns the best available size for the image.
func (i Image) SizeBytes() int64 {
	if i.Size > 0 {
		return i.Size
	}
	return i.VirtualSize
}

// ImageDetails is the response of GET /images/{name}/json.
type ImageDetails struct {
	ID            string   `json:"Id"`
	RepoTags      []string `json:"RepoTags"`
	RepoDigests   []string `json:"RepoDigests"`
	Parent        string   `json:"Parent"`
	Comment       string   `json:"Comment"`
	Created       string   `json:"Created"`
	Author        string   `json:"Author"`
	Architecture  string   `json:"Architecture"`
	Os            string   `json:"Os"`
	Size          int64    `json:"Size"`
	VirtualSize   int64    `json:"VirtualSize"`
	DockerVersion string   `json:"DockerVersion"`
	Config        *Config  `json:"Config"`
	RootFS        *struct {
		Type   string   `json:"Type"`
		Layers []string `json:"Layers"`
	} `json:"RootFS"`
}

// SizeBytes returns the best available size for the inspected image.
func (d *ImageDetails) SizeBytes() int64 {
	if d == nil {
		return 0
	}
	if d.Size > 0 {
		return d.Size
	}
	return d.VirtualSize
}

// IPAMConfig is one subnet configuration of a network.
type IPAMConfig struct {
	Subnet     string            `json:"Subnet"`
	IPRange    string            `json:"IPRange"`
	Gateway    string            `json:"Gateway"`
	AuxAddress map[string]string `json:"AuxiliaryAddresses"`
}

// IPAM is the address management configuration of a network.
type IPAM struct {
	Driver  string            `json:"Driver"`
	Options map[string]string `json:"Options"`
	Config  []IPAMConfig      `json:"Config"`
}

// NetworkContainer is a container attached to a network.
type NetworkContainer struct {
	Name        string `json:"Name"`
	EndpointID  string `json:"EndpointID"`
	MacAddress  string `json:"MacAddress"`
	IPv4Address string `json:"IPv4Address"`
	IPv6Address string `json:"IPv6Address"`
}

// Network is one entry of GET /networks.
type Network struct {
	Name       string                      `json:"Name"`
	ID         string                      `json:"Id"`
	Created    string                      `json:"Created"`
	Scope      string                      `json:"Scope"`
	Driver     string                      `json:"Driver"`
	EnableIPv6 bool                        `json:"EnableIPv6"`
	Internal   bool                        `json:"Internal"`
	Attachable bool                        `json:"Attachable"`
	Ingress    bool                        `json:"Ingress"`
	IPAM       *IPAM                       `json:"IPAM"`
	Options    map[string]string           `json:"Options"`
	Labels     map[string]string           `json:"Labels"`
	Containers map[string]NetworkContainer `json:"Containers"`
}

// DriverName returns the network driver, or a placeholder when unknown.
func (n Network) DriverName() string {
	if n.Driver == "" {
		return "-"
	}
	return n.Driver
}

// Subnets lists the configured subnets of the network.
func (n Network) Subnets() []string {
	if n.IPAM == nil {
		return nil
	}
	var out []string
	for _, c := range n.IPAM.Config {
		if c.Subnet != "" {
			out = append(out, c.Subnet)
		}
	}
	return out
}

// Gateways lists the configured gateways of the network.
func (n Network) Gateways() []string {
	if n.IPAM == nil {
		return nil
	}
	var out []string
	for _, c := range n.IPAM.Config {
		if c.Gateway != "" {
			out = append(out, c.Gateway)
		}
	}
	return out
}

// VolumeUsage is the optional usage data of a volume.
type VolumeUsage struct {
	Size     int64 `json:"Size"`
	RefCount int64 `json:"RefCount"`
}

// Volume is one entry of GET /volumes.
type Volume struct {
	Name       string            `json:"Name"`
	Driver     string            `json:"Driver"`
	Mountpoint string            `json:"Mountpoint"`
	Scope      string            `json:"Scope"`
	CreatedAt  string            `json:"CreatedAt"`
	Status     map[string]any    `json:"Status"`
	Labels     map[string]string `json:"Labels"`
	Options    map[string]string `json:"Options"`
	UsageData  *VolumeUsage      `json:"UsageData"`
}

// DriverName returns the volume driver, or a placeholder when unknown.
func (v Volume) DriverName() string {
	if v.Driver == "" {
		return "-"
	}
	return v.Driver
}

// ShortID trims a (possibly digest prefixed) Docker ID to 12 characters, which
// is what the Docker CLI displays.
func ShortID(id string) string {
	id = strings.TrimSpace(id)
	if i := strings.Index(id, ":"); i >= 0 && i < len(id)-1 {
		id = id[i+1:]
	}
	if len(id) > 12 {
		return id[:12]
	}
	return id
}

// SplitRepoTag splits an image reference such as "registry:5000/app:1.2" into
// its repository and tag parts. A missing tag yields "<none>".
func SplitRepoTag(ref string) (repo, tag string) {
	ref = strings.TrimSpace(ref)
	if ref == "" || ref == "<none>:<none>" || ref == "<none>" {
		return "<none>", "<none>"
	}
	// A colon only introduces a tag when it appears after the last slash,
	// otherwise it is the registry port.
	idx := strings.LastIndex(ref, ":")
	if idx < 0 || strings.Contains(ref[idx+1:], "/") {
		return ref, "latest"
	}
	repo, tag = ref[:idx], ref[idx+1:]
	if repo == "" {
		return ref, "latest"
	}
	if tag == "" {
		tag = "<none>"
	}
	return repo, tag
}

// FormatSizeMB renders a byte count as megabytes with exactly one decimal
// place, e.g. "143.1 MB". Docker's own CLI reports sizes in SI units (1 MB =
// 1000000 bytes) and toolm follows that convention.
func FormatSizeMB(bytes int64) string {
	mb := float64(bytes) / 1e6
	return strconv.FormatFloat(mb, 'f', 1, 64) + " MB"
}

// FormatSizeMiB renders a byte count in binary megabytes ("136.5 MiB"). It is
// shown next to the SI value in detail panes so both conventions are visible.
func FormatSizeMiB(bytes int64) string {
	mib := float64(bytes) / (1024 * 1024)
	return strconv.FormatFloat(mib, 'f', 1, 64) + " MiB"
}
