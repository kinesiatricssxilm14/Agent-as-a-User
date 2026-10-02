# toolm

`toolm` is a keyboard-driven Docker management TUI written in Go with Bubble Tea and Lip Gloss. It reads real container, image, network, and volume state directly from the Docker Engine HTTP API.

## Install

```sh
cd toolm
go install .
```

Then launch it with:

```sh
toolm
```

By default, toolm connects to `unix:///var/run/docker.sock`. Override this with `DOCKER_HOST`:

```sh
DOCKER_HOST=unix:///path/to/mock.sock toolm
DOCKER_HOST=tcp://127.0.0.1:2375 toolm
```

Supported schemes are `unix://`, `tcp://`, `http://`, and `https://`.

## Features

- Lists running and stopped containers with name, image, state, and status
- Container inspection with image, ID, command, timestamps, ports, networks, and mounts in one scrollable view
- Complete container logs with scrolling
- Images with repository, tag, exact one-decimal MB size, and image ID
- Image details
- Networks with names and drivers
- Volumes with names, drivers, and mountpoints
- Volume inspection from the Docker API
- Filtering and refresh for every resource list
- Built-in shortcut help (`?`)

## Keys

- `1`–`4`: containers, images, networks, volumes
- `Tab` / `Shift+Tab`: change resource
- `↑` / `↓`, `j` / `k`: navigate or scroll
- `PgUp` / `PgDn`, `g` / `G`: page and jump navigation
- `Enter` or `d`: details
- `l`: selected container's complete logs
- `/`: filter current list
- `r`: refresh current resource
- `Esc`: return
- `?`: full in-app help
- `q`: quit from a list; return from details/logs/help
- `Ctrl+C`: quit anywhere
