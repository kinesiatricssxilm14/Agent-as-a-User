package dockerapi

// Container is the subset of Docker's container-list response used by toolm.
type Container struct {
	ID      string          `json:"Id"`
	Names   []string        `json:"Names"`
	Image   string          `json:"Image"`
	ImageID string          `json:"ImageID"`
	Command string          `json:"Command"`
	Created int64           `json:"Created"`
	Ports   []ContainerPort `json:"Ports"`
	State   string          `json:"State"`
	Status  string          `json:"Status"`
}

type ContainerPort struct {
	IP          string `json:"IP"`
	PrivatePort uint16 `json:"PrivatePort"`
	PublicPort  uint16 `json:"PublicPort"`
	Type        string `json:"Type"`
}

type ContainerInspect struct {
	ID      string   `json:"Id"`
	Created string   `json:"Created"`
	Path    string   `json:"Path"`
	Args    []string `json:"Args"`
	Name    string   `json:"Name"`
	Config  struct {
		Hostname   string   `json:"Hostname"`
		Image      string   `json:"Image"`
		Cmd        []string `json:"Cmd"`
		Entrypoint any      `json:"Entrypoint"`
		WorkingDir string   `json:"WorkingDir"`
	} `json:"Config"`
	State struct {
		Status     string `json:"Status"`
		Running    bool   `json:"Running"`
		StartedAt  string `json:"StartedAt"`
		FinishedAt string `json:"FinishedAt"`
		ExitCode   int    `json:"ExitCode"`
	} `json:"State"`
	NetworkSettings struct {
		Ports    map[string][]PortBinding    `json:"Ports"`
		Networks map[string]EndpointSettings `json:"Networks"`
	} `json:"NetworkSettings"`
	Mounts []Mount `json:"Mounts"`
}

type PortBinding struct {
	HostIP   string `json:"HostIp"`
	HostPort string `json:"HostPort"`
}

type EndpointSettings struct {
	IPAddress  string `json:"IPAddress"`
	Gateway    string `json:"Gateway"`
	MacAddress string `json:"MacAddress"`
}

type Mount struct {
	Type        string `json:"Type"`
	Name        string `json:"Name"`
	Source      string `json:"Source"`
	Destination string `json:"Destination"`
	Mode        string `json:"Mode"`
	RW          bool   `json:"RW"`
}

type Image struct {
	ID       string   `json:"Id"`
	RepoTags []string `json:"RepoTags"`
	Size     int64    `json:"Size"`
	Created  int64    `json:"Created"`
}

type Network struct {
	Name     string `json:"Name"`
	ID       string `json:"Id"`
	Created  string `json:"Created"`
	Scope    string `json:"Scope"`
	Driver   string `json:"Driver"`
	Internal bool   `json:"Internal"`
}

type Volume struct {
	CreatedAt  string            `json:"CreatedAt"`
	Driver     string            `json:"Driver"`
	Labels     map[string]string `json:"Labels"`
	Mountpoint string            `json:"Mountpoint"`
	Name       string            `json:"Name"`
	Options    map[string]string `json:"Options"`
	Scope      string            `json:"Scope"`
}

type volumeListResponse struct {
	Volumes  []Volume `json:"Volumes"`
	Warnings []string `json:"Warnings"`
}
