package dockerapi

import (
	"context"
	"encoding/binary"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestClientResources(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/containers/json", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("all") != "1" {
			t.Errorf("all=%q", r.URL.Query().Get("all"))
		}
		fmt.Fprint(w, `[{"Id":"container123","Names":["/web"],"Image":"nginx:latest","State":"running","Status":"Up"}]`)
	})
	mux.HandleFunc("/images/json", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `[{"Id":"sha256:image123","RepoTags":["nginx:latest"],"Size":143100000}]`)
	})
	mux.HandleFunc("/networks", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `[{"Name":"bridge","Id":"network123","Driver":"bridge"}]`)
	})
	mux.HandleFunc("/volumes", func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"Volumes":[{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data/_data"}]}`)
	})
	server := httptest.NewServer(mux)
	defer server.Close()
	client := &Client{http: server.Client(), baseURL: server.URL, target: server.URL}

	containers, err := client.Containers(context.Background())
	if err != nil || len(containers) != 1 || containers[0].Image != "nginx:latest" {
		t.Fatalf("containers=%+v err=%v", containers, err)
	}
	images, err := client.Images(context.Background())
	if err != nil || len(images) != 1 || images[0].Size != 143100000 {
		t.Fatalf("images=%+v err=%v", images, err)
	}
	networks, err := client.Networks(context.Background())
	if err != nil || len(networks) != 1 || networks[0].Driver != "bridge" {
		t.Fatalf("networks=%+v err=%v", networks, err)
	}
	volumes, _, err := client.Volumes(context.Background())
	if err != nil || len(volumes) != 1 || volumes[0].Mountpoint == "" {
		t.Fatalf("volumes=%+v err=%v", volumes, err)
	}
}

func TestContainerLogsDecodesMultiplexedStream(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/containers/c1/logs", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/vnd.docker.raw-stream")
		for _, text := range []string{"one\n", "two\n"} {
			header := make([]byte, 8)
			header[0] = 1
			binary.BigEndian.PutUint32(header[4:], uint32(len(text)))
			w.Write(header)
			fmt.Fprint(w, text)
		}
	})
	server := httptest.NewServer(mux)
	defer server.Close()
	client := &Client{http: server.Client(), baseURL: server.URL, target: server.URL}
	logs, err := client.ContainerLogs(context.Background(), "c1")
	if err != nil {
		t.Fatal(err)
	}
	if logs != "one\ntwo\n" {
		t.Fatalf("logs=%q", logs)
	}
}
