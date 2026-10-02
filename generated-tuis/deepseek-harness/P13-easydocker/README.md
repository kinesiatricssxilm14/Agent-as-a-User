# toolm

A Docker container management TUI built with [Bubble Tea](https://github.com/charmbracelet/bubbletea)
and [Lip Gloss](https://github.com/charmbracelet/lipgloss). It talks to the Docker
Engine API over the local Unix socket (or `DOCKER_HOST`) and provides a unified,
keyboard-only interface for browsing and operating on Docker resources.

## Features

- **Containers** — list running and stopped containers (name, image, status,
  ports, ID), inspect full details (ID, image with tag, port mappings, command,
  mounts, networks, environment, …), and read complete logs with scrolling.
- **Images** — list name/tag, size in MB with one fixed decimal place, and
  image ID; inspect the same fields in a detail view.
- **Networks** — list name and driver; inspect driver, scope, subnets and
  attached containers.
- **Volumes** — list name and driver; inspect name, driver and absolute
  mountpoint.
- **Operations** — start / stop / restart / remove containers, remove images,
  networks and volumes (removal asks for confirmation). All operations call the
  real Docker Engine API.

## Build & install

```sh
cd toolm
go install .
```

The binary is installed as `toolm`.

## Run

```sh
toolm
```

The tool connects to `DOCKER_HOST` (e.g. `unix:///var/run/docker.sock`,
`tcp://127.0.0.1:2375`) or defaults to the local socket
`/var/run/docker.sock`.

## Keys

| Key            | Action                                           |
| -------------- | ------------------------------------------------ |
| `↑` / `↓`      | Move selection (also `k` / `j`)                  |
| `PgUp` / `PgDn`| Page through a list or scrollable view           |
| `g` / `G`      | Jump to top / bottom (also `Home` / `End`)       |
| `←` / `→`      | Switch resource view                             |
| `Tab` / `Shift+Tab` | Switch resource view                        |
| `1`-`4`        | Containers / Images / Networks / Volumes         |
| `Enter`        | Show details for the selected item               |
| `Esc`          | Back to the list / cancel                        |
| `/`            | Filter the current list                          |
| `l`            | View logs for the selected container             |
| `s` / `x` / `r`| Start / stop / restart the selected container    |
| `d`            | Remove the selected resource (asks to confirm)   |
| `ctrl+r`       | Refresh the current view                         |
| `?`            | Show / hide help                                 |
| `q` / `ctrl+c` | Quit                                             |

A context-sensitive key bar is shown at the bottom of every screen, and the
`?` key opens the full help page.
