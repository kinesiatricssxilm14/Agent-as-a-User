# toola

A keyboard-driven text user interface for system package management on Debian.
It wraps the real `apt` / `dpkg` tools so that administrators can browse,
search, install, remove and upgrade packages without memorising command-line
arguments.

## Requirements

- Go 1.22+
- A Debian-based system with `apt`, `apt-get` and `dpkg-query` available
- Root privileges (the default launch command is `sudo toola`)

## Build and install

```sh
cd toola
go install .
```

The `toola` binary is then available on your `PATH` (under `$GOPATH/bin`).
Launch it with:

```sh
sudo toola
```

## What it does

- **Browse** installed, available, upgradable, or all packages in a scrollable
  list.
- **Search** packages in real time by name or keyword.
- **Details panel** (always on the same screen, to the right of the list) shows
  the full description plus every dependency category — Depends, Pre-Depends,
  Recommends, Suggests, Conflicts, Breaks, Replaces, Provides — listed one per
  line.
- **Install / remove / upgrade** run real `apt-get` commands and stream their
  output; the package lists are refreshed afterwards so the interface always
  matches the actual system state.
- **Upgrade detection** lists upgradable packages via `apt list --upgradable`;
  after an upgrade the package disappears from the upgradable view.

## Key bindings

| Key | Action |
| --- | --- |
| `↑` / `↓` or `j` / `k` | Move selection |
| `PgUp` / `PgDn` | Page through the list |
| `Home` / `End` or `g` / `G` | Jump to first / last |
| `/` | Search / filter packages |
| `Enter` | Default action (install / remove / upgrade) |
| `Tab` | Cycle focus: list → details → search |
| `Esc` | Cancel / clear search / back to list |
| `i` | Install selected package |
| `r` | Remove (uninstall) selected package |
| `u` | Upgrade selected package |
| `U` | Upgrade all upgradable packages |
| `R` | Run `apt-get update` to refresh lists |
| `1` `2` `3` `4` | Switch view: installed / available / upgradable / all |
| `?` | Toggle this help overlay |
| `q` / `Ctrl+C` | Quit |

All key bindings are also discoverable from the in-app help overlay (`?`) and
the bottom status/help bar.

## Design notes

- Every read/write of system state calls the real underlying tools
  (`dpkg-query`, `apt-cache`, `apt list`, `apt-get`). Nothing is simulated.
- Operations are confirmed inline and run with
  `DEBIAN_FRONTEND=noninteractive` plus non-interactive dpkg config handling so
  they never hang waiting for input.
- Package lists are loaded asynchronously on start and refreshed after every
  mutation.
