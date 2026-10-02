package docker

import (
	"context"
	"encoding/binary"
	"net"
	"net/http"
	"path/filepath"
	"strings"
	"testing"
)

// startMockDocker serves a minimal Docker Engine API over a Unix socket so the
// client can be exercised without a real daemon.
func startMockDocker(t *testing.T) (socket string, cleanup func()) {
	t.Helper()
	socket = filepath.Join(t.TempDir(), "docker.sock")
	ln, err := net.Listen("unix", socket)
	if err != nil {
		t.Fatal(err)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		p := r.URL.Path
		switch {
		case p == "/containers/json":
			_, _ = w.Write([]byte(`[
				{"Id":"abc123def456abc123def456abc123def456","Names":["/web"],"Image":"nginx:latest","ImageID":"sha256:abc123","Command":"nginx","Created":1700000000,"Ports":[{"IP":"0.0.0.0","PrivatePort":80,"PublicPort":8080,"Type":"tcp"}],"State":"running","Status":"Up 3 hours"}
			]`))
		case p == "/containers/abc123def456/json":
			_, _ = w.Write([]byte(`{
				"Id":"abc123def456abc123def456abc123def456",
				"Created":"2024-01-01T00:00:00Z",
				"Path":"nginx",
				"Args":["-g","daemon off;"],
				"Name":"/web",
				"Image":"nginx:latest",
				"Config":{"Image":"nginx:latest","Cmd":["nginx","-g","daemon off;"],"Entrypoint":[],"Env":["PATH=/usr/bin"],"Tty":false},
				"State":{"Status":"running","Running":true,"ExitCode":0,"StartedAt":"2024-01-01T00:00:00Z","Pid":42},
				"HostConfig":{"PortBindings":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}]},"NetworkMode":"bridge"},
				"NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}]},"Networks":{"bridge":{"IPAddress":"172.17.0.2"}}},
				"Mounts":[]
			}`))
		case p == "/containers/abc123def456/logs":
			w.Header().Set("Content-Type", "application/vnd.docker.raw-stream")
			msg := "hello world\n"
			frame := []byte{1, 0, 0, 0} // stdout stream
			size := make([]byte, 4)
			binary.BigEndian.PutUint32(size, uint32(len(msg)))
			frame = append(frame, size...)
			frame = append(frame, msg...)
			_, _ = w.Write(frame)
		case p == "/images/json":
			_, _ = w.Write([]byte(`[
				{"Id":"sha256:img1234567890","RepoTags":["nginx:latest"],"RepoDigests":["nginx@sha256:digest"],"Size":150000000,"Created":1700000000}
			]`))
		case p == "/images/sha256:img1234567890/json":
			_, _ = w.Write([]byte(`{
				"Id":"sha256:img1234567890",
				"RepoTags":["nginx:latest"],
				"RepoDigests":["nginx@sha256:digest"],
				"Size":150000000,
				"Created":"2024-01-01T00:00:00Z",
				"Architecture":"amd64",
				"Os":"linux",
				"Config":{"Cmd":["nginx","-g","daemon off;"],"Entrypoint":[],"Env":["PATH=/usr/bin"]}
			}`))
		case p == "/networks":
			_, _ = w.Write([]byte(`[
				{"Name":"bridge","Id":"net1","Driver":"bridge","Scope":"local","Internal":false,"Attachable":false}
			]`))
		case p == "/networks/net1":
			_, _ = w.Write([]byte(`{
				"Name":"bridge","Id":"net1","Driver":"bridge","Scope":"local","Internal":false,"Attachable":false,
				"IPAM":{"Driver":"default","Config":[{"Subnet":"172.17.0.0/16","Gateway":"172.17.0.1"}]},
				"Containers":{},"Options":{}
			}`))
		case p == "/volumes":
			_, _ = w.Write([]byte(`{"Volumes":[
				{"Name":"myvol","Driver":"local","Mountpoint":"/var/lib/docker/volumes/myvol/_data","CreatedAt":"2024-01-01T00:00:00Z","Scope":"local"}
			],"Warnings":[]}`))
		case p == "/volumes/myvol":
			_, _ = w.Write([]byte(`{
				"Name":"myvol","Driver":"local","Mountpoint":"/var/lib/docker/volumes/myvol/_data","CreatedAt":"2024-01-01T00:00:00Z","Scope":"local"
			}`))
		default:
			http.NotFound(w, r)
		}
	})

	srv := &http.Server{Handler: mux}
	go func() { _ = srv.Serve(ln) }()
	return socket, func() {
		_ = srv.Close()
		_ = ln.Close()
	}
}

func newTestClient(t *testing.T) *Client {
	t.Helper()
	socket, cleanup := startMockDocker(t)
	t.Cleanup(cleanup)
	t.Setenv("DOCKER_HOST", "unix://"+socket)
	return New()
}

func TestListContainers(t *testing.T) {
	c := newTestClient(t)
	items, err := c.ListContainers(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(items) != 1 {
		t.Fatalf("want 1 container, got %d", len(items))
	}
	if items[0].Name() != "web" {
		t.Errorf("name = %q, want web", items[0].Name())
	}
	if items[0].Image != "nginx:latest" {
		t.Errorf("image = %q", items[0].Image)
	}
}

func TestInspectContainer(t *testing.T) {
	c := newTestClient(t)
	insp, err := c.InspectContainer(context.Background(), "abc123def456")
	if err != nil {
		t.Fatal(err)
	}
	if insp.Name != "/web" {
		t.Errorf("name = %q", insp.Name)
	}
	if insp.Config == nil || insp.Config.Image != "nginx:latest" {
		t.Errorf("config image = %v", insp.Config)
	}
}

func TestContainerLogs(t *testing.T) {
	c := newTestClient(t)
	logs, err := c.ContainerLogs(context.Background(), "abc123def456")
	if err != nil {
		t.Fatal(err)
	}
	if logs != "hello world\n" {
		t.Errorf("logs = %q", logs)
	}
}

func TestListImagesAndVolumes(t *testing.T) {
	c := newTestClient(t)
	imgs, err := c.ListImages(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(imgs) != 1 || imgs[0].RepoTags[0] != "nginx:latest" {
		t.Errorf("unexpected images: %+v", imgs)
	}
	vols, err := c.ListVolumes(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(vols) != 1 || vols[0].Mountpoint != "/var/lib/docker/volumes/myvol/_data" {
		t.Errorf("unexpected volumes: %+v", vols)
	}
}

func TestDemuxLogsRawFallback(t *testing.T) {
	raw := "plain text output\n"
	got, err := demuxLogs(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	if got != raw {
		t.Errorf("got %q, want %q", got, raw)
	}
}
