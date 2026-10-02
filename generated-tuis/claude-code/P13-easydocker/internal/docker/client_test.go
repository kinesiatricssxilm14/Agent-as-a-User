package docker

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestFormatSizeMBAlwaysOneDecimal(t *testing.T) {
	cases := []struct {
		bytes int64
		want  string
	}{
		{143100000, "143.1 MB"},
		{256000000, "256.0 MB"},
		{0, "0.0 MB"},
		{1, "0.0 MB"},
		{999999, "1.0 MB"},
		{1500000, "1.5 MB"},
		{7340032000, "7340.0 MB"},
	}
	for _, c := range cases {
		if got := FormatSizeMB(c.bytes); got != c.want {
			t.Errorf("FormatSizeMB(%d) = %q, want %q", c.bytes, got, c.want)
		}
	}
}

func TestSplitRepoTag(t *testing.T) {
	cases := []struct {
		ref, repo, tag string
	}{
		{"nginx:1.25", "nginx", "1.25"},
		{"nginx", "nginx", "latest"},
		{"library/redis:7-alpine", "library/redis", "7-alpine"},
		{"registry.example.com:5000/team/app:v2", "registry.example.com:5000/team/app", "v2"},
		{"registry.example.com:5000/team/app", "registry.example.com:5000/team/app", "latest"},
		{"<none>:<none>", "<none>", "<none>"},
		{"", "<none>", "<none>"},
	}
	for _, c := range cases {
		repo, tag := SplitRepoTag(c.ref)
		if repo != c.repo || tag != c.tag {
			t.Errorf("SplitRepoTag(%q) = (%q, %q), want (%q, %q)", c.ref, repo, tag, c.repo, c.tag)
		}
	}
}

func TestShortID(t *testing.T) {
	cases := []struct{ in, want string }{
		{"sha256:0123456789abcdef0123", "0123456789ab"},
		{"0123456789abcdef", "0123456789ab"},
		{"abc", "abc"},
		{"", ""},
	}
	for _, c := range cases {
		if got := ShortID(c.in); got != c.want {
			t.Errorf("ShortID(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

// frame builds a Docker multiplexed log frame.
func frame(stream byte, payload string) []byte {
	buf := make([]byte, 8+len(payload))
	buf[0] = stream
	binary.BigEndian.PutUint32(buf[4:8], uint32(len(payload)))
	copy(buf[8:], payload)
	return buf
}

func TestDemuxLogsFramed(t *testing.T) {
	data := append(frame(1, "hello\n"), frame(2, "warning\n")...)
	if got := string(DemuxLogs(data)); got != "hello\nwarning\n" {
		t.Errorf("DemuxLogs framed = %q", got)
	}
}

func TestDemuxLogsPlainTextPassthrough(t *testing.T) {
	// A plain (TTY style) log must survive untouched, even when it is long.
	plain := "starting server\nlistening on :80\n"
	if got := string(DemuxLogs([]byte(plain))); got != plain {
		t.Errorf("DemuxLogs plain = %q, want %q", got, plain)
	}
}

func TestSplitLogLines(t *testing.T) {
	got := SplitLogLines("a\r\nb\rc\n")
	want := []string{"a", "b", "c"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("SplitLogLines = %#v, want %#v", got, want)
	}
	if lines := SplitLogLines(""); len(lines) != 0 {
		t.Errorf("SplitLogLines(\"\") = %#v, want empty", lines)
	}
}

func TestContainerHelpers(t *testing.T) {
	c := Container{
		ID:     "sha256:deadbeefcafebabe",
		Names:  []string{"/web", "/web-alias"},
		Image:  "nginx:1.25",
		Status: "Up 3 hours",
		Ports: []Port{
			{IP: "0.0.0.0", PublicPort: 8080, PrivatePort: 80, Type: "tcp"},
			{PrivatePort: 443, Type: "tcp"},
		},
	}
	if c.Name() != "web" {
		t.Errorf("Name() = %q", c.Name())
	}
	if got, want := c.PortsString(), "0.0.0.0:8080->80/tcp, 443/tcp"; got != want {
		t.Errorf("PortsString() = %q, want %q", got, want)
	}
	if !c.IsRunning() {
		t.Error("IsRunning() = false for a container with status Up")
	}
	if c.StateLabel() != "running" {
		t.Errorf("StateLabel() = %q, want running", c.StateLabel())
	}
}

func TestStringListAcceptsStringOrArray(t *testing.T) {
	var cfg Config
	if err := unmarshal(`{"Cmd":"nginx -g daemon off;","Entrypoint":["/docker-entrypoint.sh"]}`, &cfg); err != nil {
		t.Fatal(err)
	}
	if got := cfg.Cmd.String(); got != "nginx -g daemon off;" {
		t.Errorf("Cmd = %q", got)
	}
	if got := cfg.Entrypoint.String(); got != "/docker-entrypoint.sh" {
		t.Errorf("Entrypoint = %q", got)
	}
}

func TestContainerDetailsPortMappings(t *testing.T) {
	var d ContainerDetails
	err := unmarshal(`{
		"Id":"abc","Name":"/web","Path":"nginx","Args":["-g","daemon off;"],
		"NetworkSettings":{"Ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}],"443/tcp":null}},
		"Config":{"ExposedPorts":{"80/tcp":{},"8443/tcp":{}}}
	}`, &d)
	if err != nil {
		t.Fatal(err)
	}
	// 80/tcp is published, so it must appear once as a mapping and not again as
	// a bare exposed port. 443/tcp is exposed only, 8443/tcp comes from Config.
	got := d.PortMappings()
	want := []string{"0.0.0.0:8080->80/tcp", "443/tcp", "8443/tcp"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("PortMappings() = %#v, want %#v", got, want)
	}
	if d.CleanName() != "web" {
		t.Errorf("CleanName() = %q", d.CleanName())
	}
	if got, want := d.CommandLine(), "nginx -g daemon off;"; got != want {
		t.Errorf("CommandLine() = %q, want %q", got, want)
	}
}

// unmarshal is a tiny helper so tests read clearly.
func unmarshal(s string, v any) error {
	return json.Unmarshal([]byte(s), v)
}

// --- API client tests against an httptest server over a unix socket ---

// mockHandler serves a small, fixed Docker API surface.
func mockHandler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/_ping", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("OK"))
	})
	mux.HandleFunc("/version", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{"Version":"24.0.7","ApiVersion":"1.43"}`))
	})
	mux.HandleFunc("/containers/json", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`[{"Id":"c1","Names":["/web"],"Image":"nginx:1.25","State":"running","Status":"Up 2 hours",
			"Ports":[{"IP":"0.0.0.0","PrivatePort":80,"PublicPort":8080,"Type":"tcp"}]}]`))
	})
	mux.HandleFunc("/containers/c1/json", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{"Id":"c1","Name":"/web","Path":"nginx","Config":{"Image":"nginx:1.25"}}`))
	})
	mux.HandleFunc("/containers/c1/logs", func(w http.ResponseWriter, r *http.Request) {
		w.Write(frame(1, "log line one\n"))
	})
	mux.HandleFunc("/images/json", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`[{"Id":"sha256:img1","RepoTags":["nginx:1.25"],"Size":143100000}]`))
	})
	mux.HandleFunc("/images/nginx:1.25/json", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{"Id":"sha256:img1","RepoTags":["nginx:1.25"],"Size":143100000,"Os":"linux"}`))
	})
	mux.HandleFunc("/networks", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`[{"Name":"bridge","Id":"n1","Driver":"bridge","Scope":"local"}]`))
	})
	mux.HandleFunc("/volumes", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{"Volumes":[{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data/_data"}]}`))
	})
	mux.HandleFunc("/volumes/data", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data/_data","Scope":"local"}`))
	})
	return mux
}

// startUnixMock serves the mock API on a unix socket in a temp dir.
func startUnixMock(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	sock := filepath.Join(dir, "docker.sock")
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Skipf("unix sockets unavailable: %v", err)
	}
	srv := &httptest.Server{
		Listener: ln,
		Config:   &http.Server{Handler: mockHandler()},
	}
	srv.Start()
	t.Cleanup(srv.Close)
	return sock
}

func TestAPIClientAgainstMockSocket(t *testing.T) {
	sock := startUnixMock(t)
	c, err := NewAPIClient("unix://" + sock)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()

	if err := c.ping(ctx); err != nil {
		t.Fatalf("ping: %v", err)
	}
	if v, err := c.Version(ctx); err != nil || v != "24.0.7 (API 1.43)" {
		t.Fatalf("Version() = %q, %v", v, err)
	}

	containers, err := c.Containers(ctx)
	if err != nil || len(containers) != 1 {
		t.Fatalf("Containers() = %#v, %v", containers, err)
	}
	if containers[0].Name() != "web" || containers[0].Image != "nginx:1.25" {
		t.Errorf("unexpected container %#v", containers[0])
	}

	if d, err := c.ContainerInspect(ctx, "c1"); err != nil || d.CleanName() != "web" {
		t.Fatalf("ContainerInspect = %#v, %v", d, err)
	}
	if logs, err := c.ContainerLogs(ctx, "c1"); err != nil || logs != "log line one\n" {
		t.Fatalf("ContainerLogs = %q, %v", logs, err)
	}

	images, err := c.Images(ctx)
	if err != nil || len(images) != 1 || FormatSizeMB(images[0].SizeBytes()) != "143.1 MB" {
		t.Fatalf("Images() = %#v, %v", images, err)
	}
	if d, err := c.ImageInspect(ctx, "nginx:1.25"); err != nil || d.Os != "linux" {
		t.Fatalf("ImageInspect = %#v, %v", d, err)
	}

	nets, err := c.Networks(ctx)
	if err != nil || len(nets) != 1 || nets[0].DriverName() != "bridge" {
		t.Fatalf("Networks() = %#v, %v", nets, err)
	}

	vols, err := c.Volumes(ctx)
	if err != nil || len(vols) != 1 {
		t.Fatalf("Volumes() = %#v, %v", vols, err)
	}
	if vols[0].Mountpoint != "/var/lib/docker/volumes/data/_data" {
		t.Errorf("volume mountpoint = %q", vols[0].Mountpoint)
	}
	if v, err := c.VolumeInspect(ctx, "data"); err != nil || v.Scope != "local" {
		t.Fatalf("VolumeInspect = %#v, %v", v, err)
	}
}

func TestVolumesAcceptsBareArray(t *testing.T) {
	dir := t.TempDir()
	sock := filepath.Join(dir, "d.sock")
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Skipf("unix sockets unavailable: %v", err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/volumes", func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte(`[{"Name":"cache","Driver":"local","Mountpoint":"/mnt/cache"}]`))
	})
	srv := &httptest.Server{Listener: ln, Config: &http.Server{Handler: mux}}
	srv.Start()
	defer srv.Close()

	c, err := NewAPIClient("unix://" + sock)
	if err != nil {
		t.Fatal(err)
	}
	vols, err := c.Volumes(context.Background())
	if err != nil || len(vols) != 1 || vols[0].Name != "cache" {
		t.Fatalf("Volumes() = %#v, %v", vols, err)
	}
}

func TestAPIErrorIsReported(t *testing.T) {
	dir := t.TempDir()
	sock := filepath.Join(dir, "d.sock")
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Skipf("unix sockets unavailable: %v", err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/containers/json", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
		w.Write([]byte(`{"message":"daemon is unwell"}`))
	})
	srv := &httptest.Server{Listener: ln, Config: &http.Server{Handler: mux}}
	srv.Start()
	defer srv.Close()

	c, _ := NewAPIClient("unix://" + sock)
	_, err = c.Containers(context.Background())
	if err == nil {
		t.Fatal("expected an error")
	}
	if want := "daemon is unwell"; !strings.Contains(err.Error(), want) {
		t.Errorf("error %q does not mention %q", err, want)
	}
}

func TestNewClientPrefersDockerHostEnv(t *testing.T) {
	sock := startUnixMock(t)
	t.Setenv("DOCKER_HOST", "unix://"+sock)

	c, err := NewClient(context.Background())
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if c.Endpoint() != "unix://"+sock {
		t.Errorf("Endpoint() = %q, want the DOCKER_HOST socket", c.Endpoint())
	}
	if _, err := c.Containers(context.Background()); err != nil {
		t.Errorf("Containers: %v", err)
	}
}

func TestNewClientFailsWithoutEndpoint(t *testing.T) {
	// Point DOCKER_HOST at a path that cannot exist and hide any docker CLI.
	t.Setenv("DOCKER_HOST", "unix:///nonexistent/toolm-test.sock")
	t.Setenv("PATH", t.TempDir())
	if _, err := os.Stat("/var/run/docker.sock"); err == nil {
		t.Skip("a real docker socket is present on this host")
	}
	if _, err := NewClient(context.Background()); err == nil {
		t.Fatal("expected NewClient to fail with no reachable endpoint")
	}
}
