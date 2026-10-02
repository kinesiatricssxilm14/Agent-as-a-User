// Package docker implements a minimal client for the Docker Engine API over a
// local Unix socket (or a TCP endpoint configured via DOCKER_HOST).
package docker

// Container is a summary entry from GET /containers/json.
type Container struct {
	ID      string   `json:"Id"`
	Names   []string `json:"Names"`
	Image   string   `json:"Image"`
	ImageID string   `json:"ImageID"`
	Command string   `json:"Command"`
	Created int64    `json:"Created"`
	Ports   []Port   `json:"Ports"`
	State   string   `json:"State"`
	Status  string   `json:"Status"`
}

// Name returns the first container name with its leading slash removed.
func (c Container) Name() string {
	if len(c.Names) == 0 {
		return ""
	}
	return trimSlash(c.Names[0])
}

func trimSlash(s string) string {
	if len(s) > 0 && s[0] == '/' {
		return s[1:]
	}
	return s
}

// Port is a container port mapping entry.
type Port struct {
	IP          string `json:"IP"`
	PrivatePort uint16 `json:"PrivatePort"`
	PublicPort  uint16 `json:"PublicPort"`
	Type        string `json:"Type"`
}

// ContainerInspect is the result of GET /containers/{id}/json.
type ContainerInspect struct {
	ID              string           `json:"Id"`
	Created         string           `json:"Created"`
	Path            string           `json:"Path"`
	Args            []string         `json:"Args"`
	Name            string           `json:"Name"`
	Image           string           `json:"Image"`
	Config          *ContainerConfig `json:"Config"`
	State           *ContainerState  `json:"State"`
	HostConfig      *HostConfig      `json:"HostConfig"`
	NetworkSettings *NetworkSettings `json:"NetworkSettings"`
	Mounts          []Mount          `json:"Mounts"`
}

// ContainerConfig mirrors the container's runtime configuration.
type ContainerConfig struct {
	Image        string              `json:"Image"`
	Cmd          []string            `json:"Cmd"`
	Entrypoint   []string            `json:"Entrypoint"`
	Env          []string            `json:"Env"`
	WorkingDir   string              `json:"WorkingDir"`
	Tty          bool                `json:"Tty"`
	Labels       map[string]string   `json:"Labels"`
	ExposedPorts map[string]struct{} `json:"ExposedPorts"`
}

// ContainerState mirrors the container state.
type ContainerState struct {
	Status     string `json:"Status"`
	Running    bool   `json:"Running"`
	Paused     bool   `json:"Paused"`
	Restarting bool   `json:"Restarting"`
	StartedAt  string `json:"StartedAt"`
	FinishedAt string `json:"FinishedAt"`
	ExitCode   int    `json:"ExitCode"`
	Error      string `json:"Error"`
	OOMKilled  bool   `json:"OOMKilled"`
	Pid        int    `json:"Pid"`
}

// HostConfig mirrors the host configuration.
type HostConfig struct {
	PortBindings  map[string][]PortBinding `json:"PortBindings"`
	NetworkMode   string                   `json:"NetworkMode"`
	RestartPolicy RestartPolicy            `json:"RestartPolicy"`
}

// RestartPolicy mirrors the restart policy.
type RestartPolicy struct {
	Name string `json:"Name"`
}

// PortBinding maps a container port to a host port.
type PortBinding struct {
	HostIP   string `json:"HostIp"`
	HostPort string `json:"HostPort"`
}

// NetworkEndpoint is a container's endpoint inside one network.
type NetworkEndpoint struct {
	IPAddress  string `json:"IPAddress"`
	Gateway    string `json:"Gateway"`
	MacAddress string `json:"MacAddress"`
}

// NetworkSettings holds network and port information.
type NetworkSettings struct {
	Ports     map[string][]PortBinding   `json:"Ports"`
	Networks  map[string]NetworkEndpoint `json:"Networks"`
	IPAddress string                     `json:"IPAddress"`
}

// Mount describes a filesystem mount.
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

// Image is a summary entry from GET /images/json.
type Image struct {
	ID          string            `json:"Id"`
	RepoTags    []string          `json:"RepoTags"`
	RepoDigests []string          `json:"RepoDigests"`
	Size        int64             `json:"Size"`
	VirtualSize int64             `json:"VirtualSize"`
	Created     int64             `json:"Created"`
	Containers  int               `json:"Containers"`
	Labels      map[string]string `json:"Labels"`
}

// ImageInspect is the result of GET /images/{id}/json.
type ImageInspect struct {
	ID           string       `json:"Id"`
	RepoTags     []string     `json:"RepoTags"`
	RepoDigests  []string     `json:"RepoDigests"`
	Size         int64        `json:"Size"`
	Created      string       `json:"Created"`
	Architecture string       `json:"Architecture"`
	Os           string       `json:"Os"`
	Config       *ImageConfig `json:"Config"`
	Container    string       `json:"Container"`
}

// ImageConfig mirrors the image configuration.
type ImageConfig struct {
	Env          []string            `json:"Env"`
	Cmd          []string            `json:"Cmd"`
	Entrypoint   []string            `json:"Entrypoint"`
	WorkingDir   string              `json:"WorkingDir"`
	ExposedPorts map[string]struct{} `json:"ExposedPorts"`
	Labels       map[string]string   `json:"Labels"`
}

// Network is a summary entry from GET /networks.
type Network struct {
	Name       string `json:"Name"`
	ID         string `json:"Id"`
	Driver     string `json:"Driver"`
	Scope      string `json:"Scope"`
	Internal   bool   `json:"Internal"`
	Attachable bool   `json:"Attachable"`
	Ingress    bool   `json:"Ingress"`
}

// NetworkInspect is the result of GET /networks/{id}.
type NetworkInspect struct {
	Name       string                      `json:"Name"`
	ID         string                      `json:"Id"`
	Driver     string                      `json:"Driver"`
	Scope      string                      `json:"Scope"`
	Internal   bool                        `json:"Internal"`
	Attachable bool                        `json:"Attachable"`
	Ingress    bool                        `json:"Ingress"`
	IPAM       IPAM                        `json:"IPAM"`
	Containers map[string]NetworkContainer `json:"Containers"`
	Options    map[string]string           `json:"Options"`
	Labels     map[string]string           `json:"Labels"`
}

// IPAM holds IP address management configuration.
type IPAM struct {
	Driver string       `json:"Driver"`
	Config []IPAMConfig `json:"Config"`
}

// IPAMConfig holds a subnet definition.
type IPAMConfig struct {
	Subnet  string `json:"Subnet"`
	Gateway string `json:"Gateway"`
	IPRange string `json:"IPRange"`
}

// NetworkContainer is a container attached to a network.
type NetworkContainer struct {
	Name        string `json:"Name"`
	IPv4Address string `json:"IPv4Address"`
	IPv6Address string `json:"IPv6Address"`
	MacAddress  string `json:"MacAddress"`
}

// VolumeListResponse is the result of GET /volumes.
type VolumeListResponse struct {
	Volumes  []Volume `json:"Volumes"`
	Warnings []string `json:"Warnings"`
}

// Volume describes a Docker volume.
type Volume struct {
	Name       string            `json:"Name"`
	Driver     string            `json:"Driver"`
	Mountpoint string            `json:"Mountpoint"`
	CreatedAt  string            `json:"CreatedAt"`
	Scope      string            `json:"Scope"`
	Labels     map[string]string `json:"Labels"`
	Options    map[string]string `json:"Options"`
}
