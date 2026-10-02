# toolm

A keyboard-driven terminal interface for inspecting Docker resources:
containers, images, networks and volumes.

`toolm` reads live state from a real Docker endpoint and renders it in a single
screen per resource, so everything relevant is visible at once without opening
modals or switching between hidden panels.

## Features

- **Containers** — every container, running and stopped, with name, image,
  state, status, published ports and container ID on one screen.
- **Container details** — name, image with tag, port mappings, start command,
  ID, state, networks, mounts, environment and labels.
- **Container logs** — the complete log output of a container, scrollable, with
  line numbers and an optional soft-wrap mode.
- **Images** — repository, tag, size in megabytes (always one decimal place,
  e.g. `143.1 MB`), image ID and age; a detail view adds build metadata,
  configuration and layers.
- **Networks** — name, driver, scope, ID and subnet, plus a detail view with
  IPAM configuration and connected containers.
- **Volumes** — name, driver, mountpoint and scope, plus a detail view with the
  absolute mountpoint, usage data, options and labels.
- Live filtering, sortable columns, and error messages that name the endpoint
  that failed.

## Install

```sh
cd toolm && go install .
```

This produces the `toolm` command in `$(go env GOPATH)/bin`. Start it with no
arguments:

```sh
toolm
```

## Connecting to Docker

`toolm` never fabricates data; every screen reflects a real API response. It
locates an endpoint in this order:

1. `DOCKER_HOST`, if set — `unix:///path/to/docker.sock`, `tcp://host:2375`, or
   an `http(s)://` URL. A mock socket that speaks the Docker API works here too.
2. The standard local socket paths: `/var/run/docker.sock`, `/run/docker.sock`.
3. The `docker` CLI, as a fallback, for hosts where only the client is
   configured.

The active endpoint and engine version are shown in the header. If nothing can
be reached, `toolm` exits with a message listing every endpoint it tried.

## Keys

Press `?` inside the interface for the full, scrollable reference. The bottom
line always shows the bindings that apply to the current screen.

| Key | Action |
| --- | --- |
| `tab` / `shift+tab` | next / previous view |
| `1` `2` `3` `4`, or `c` `i` `n` `v` | jump to Containers / Images / Networks / Volumes |
| `↑` `↓` or `k` `j` | move the selection |
| `pgup` / `pgdn`, `home` / `end` | page and jump |
| `enter` | details for the selected item |
| `l` | logs of the selected container |
| `/` | filter the current list (`enter` applies, `esc` clears) |
| `s` / `S` | cycle sort column / reverse order |
| `r` | reload from the Docker endpoint |
| `w` | toggle line wrapping in the log viewer |
| `esc` | leave details, logs, help or the filter |
| `?` | key reference |
| `q` / `ctrl+c` | quit |

Filtering accepts several space separated terms, all of which must match — for
example `nginx running` in the container view.

## Development

```sh
go test ./...     # unit tests and UI interaction tests
go vet ./...
```

The tests drive the Bubble Tea model through real key presses and assert on
rendered output, including that no screen ever exceeds the terminal bounds. The
Docker client is tested against an HTTP server on a temporary unix socket.

## Layout

```
toolm/
├── main.go                  entry point, endpoint discovery, CLI flags
└── internal/
    ├── docker/              Docker API client and data model
    │   ├── types.go         API types, size and reference formatting
    │   ├── client.go        socket/TCP API client, log de-multiplexing
    │   └── cli.go           docker CLI fallback client
    └── ui/                  Bubble Tea interface
        ├── model.go         state, key handling, filtering and sorting
        ├── view.go          layout and rendering of every screen
        ├── details.go       detail pane field builders
        ├── keys.go          key documentation
        ├── layout.go        width-aware table, wrapping and scrollbars
        └── styles.go        Lip Gloss styles
```

Built with [Bubble Tea](https://github.com/charmbracelet/bubbletea) and
[Lip Gloss](https://github.com/charmbracelet/lipgloss).
