package tui

import (
	"strings"
	"testing"

	"toolm/dockerapi"
)

func TestImageSizeOneDecimalMegabytes(t *testing.T) {
	if got := imageSize(143100000); got != "143.1 MB" {
		t.Fatalf("got %q", got)
	}
	if got := imageSize(256000000); got != "256.0 MB" {
		t.Fatalf("got %q", got)
	}
}

func TestDetailsContainRequiredFields(t *testing.T) {
	m := Model{active: volumes}
	m.volumeDetail = &dockerapi.Volume{Name: "data", Driver: "local", Mountpoint: "/absolute/path"}
	joined := strings.Join(m.detailLines(), "\n")
	for _, want := range []string{"Name:", "data", "Driver:", "local", "Mountpoint:", "/absolute/path"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("details missing %q: %s", want, joined)
		}
	}
}

func TestContainerListHasNameAndImage(t *testing.T) {
	m := Model{width: 120, containerItems: []dockerapi.Container{{Names: []string{"/web"}, Image: "nginx:latest", State: "running", Status: "Up"}}}
	header, rows := m.containerRows()
	joined := header + "\n" + strings.Join(rows, "\n")
	for _, want := range []string{"NAME", "IMAGE", "web", "nginx:latest"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("list missing %q: %s", want, joined)
		}
	}
}
