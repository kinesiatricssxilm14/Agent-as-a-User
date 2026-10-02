# toola

`toola` is a keyboard-driven Debian package management TUI. It uses the real
`apt`, `apt-get`, `apt-cache`, and `dpkg-query` commands; package operations are
not simulated.

## Install

On Debian 12 (or another apt-based system), with Go installed:

```sh
cd toola
go install .
sudo "$(go env GOPATH)/bin/toola"
```

If the Go binary directory is already on root's `PATH`, launch it as requested:

```sh
sudo toola
```

## Keys

| Key | Action |
|---|---|
| `↑`/`↓`, `j`/`k` | Select a package |
| `PgUp`/`PgDn`, `g`/`G` | Move through the package list |
| `/` | Start real-time package name/description keyword filtering |
| `Enter` | Finish search or confirm an operation |
| `Esc` | Clear/cancel/return to details |
| `1` | Show all packages |
| `2` | Show installed packages |
| `3` | Show available, not-installed packages |
| `4` | Show upgradable packages |
| `Ctrl+U`/`Ctrl+D`, `←`/`→` | Scroll the fixed details panel |
| `i` | Install the selected available package |
| `x` | Remove the selected installed package |
| `u` | Upgrade the selected upgradable package |
| `U` | Upgrade all upgradable packages |
| `a` | Run `apt-get update` |
| `r` | Reload current package state |
| `o` | Jump between package details and the most recent apt output |
| `q` | Quit |

All mutating operations require an explicit `y`/`Enter` confirmation. After an
install, removal, or upgrade, toola reloads package state and details so the UI
matches apt's actual state.
