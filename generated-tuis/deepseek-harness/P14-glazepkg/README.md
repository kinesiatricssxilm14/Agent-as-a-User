# tooln

`tooln` is a keyboard-driven text user interface (TUI) for managing Python
(`pip`) and system (`apt`) packages inside a Debian container. It offers a
lazygit-style layout: a package list on the left and a live details pane on the
right, with discoverable shortcuts shown in the footer and a built-in help page.

All state is read from and written to the real package managers — `tooln` never
simulates anything:

- pip operations run through `python3 -m pip`
- apt operations run through `apt-get`, `apt-cache` and `dpkg`/`dpkg-query`
- package search queries the live PyPI index (for pip) or the apt cache

## Building

```sh
cd tooln
go install .
```

Dependencies are vendored, so the build does not require network access. The
installed binary is named `tooln`.

## Running

```sh
tooln
```

The tool is designed to run as root inside a Debian 12 slim container that has
`python3`, `python3-pip` and `python3-venv` installed.

## Features

| Capability | Notes |
| --- | --- |
| Multi-manager | Switch between the `pip` and `apt` views |
| pip list | Installed Python packages and versions |
| apt list | Installed system packages and versions |
| Install pip package | Search PyPI, select, confirm, install; dependencies appear in list/details |
| Uninstall pip package | Removes the package and any now-orphaned dependencies |
| Uninstall apt package | Removes the selected system package |
| Upgrade pip package | Upgrades in place; the new version is shown |
| Install apt package | Search the apt cache, select, install |
| Refresh | Rescans the environment after every operation (and on `r`) |

## Keyboard shortcuts

The footer and the `?` help page document every binding. Core bindings:

| Key | Action |
| --- | --- |
| `tab` / `shift+tab` (also `←` / `→`) | switch pip ↔ apt view |
| `↑` / `k`, `↓` / `j` | move selection |
| `pgup` / `pgdn`, `g` / `G` | page / jump to top / bottom |
| `/` | filter the current list (type to filter) |
| `s` or `i` | search repository and install |
| `d` or `x` | uninstall selected package |
| `u` | upgrade selected package |
| `r` | refresh list |
| `enter` | focus the details pane (`↑`/`↓` scroll, `esc` back) |
| `?` | help page |
| `q` / `ctrl+c` | quit |
| `enter` / `y` vs `esc` / `n` | confirm vs cancel an action |

## Project layout

```
tooln/
├── main.go                       # entry point
├── go.mod / go.sum               # module definition
├── vendor/                       # vendored third-party libraries
└── internal/
    ├── pkgs/                     # shared types and Manager interface
    ├── runner/                   # subprocess runner
    ├── manager/                  # pip, apt and PyPI search backends
    │   └── manager_test.go       # unit tests for parsing / autoremove logic
    └── ui/                       # Bubble Tea model, rendering and keymap
```
