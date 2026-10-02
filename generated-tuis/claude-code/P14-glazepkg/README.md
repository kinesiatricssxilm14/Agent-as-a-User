# tooln

A terminal interface for managing the packages installed on a machine: the
Python distributions `pip` knows about, and the Debian system packages `dpkg`
and `apt` know about, in one keyboard-driven view.

`tooln` is a front end, not a reimplementation. Every figure it shows and every
change it makes goes through the real tool — `pip list`, `pip show`,
`dpkg-query`, `apt-cache`, `pip install`, `apt-get remove`, and so on. Press
`L` at any time to see the exact commands it ran and what they printed.

```
tooln · Python & system package manager
 1 pip 54    2 apt 131                                       tab switches · ? help
╭─────────────────────────────────────────────────────╮╭──────────────────────────╮
│ pip installed                                 54/54 ││ Details: Flask           │
│    PACKAGE          VERSION   NEWER  DESCRIPTION    ││ Name:                    │
│ ›  Flask            3.0.0     3.1.3  A simple fra…  ││   Flask                  │
│    blinker          1.9.0            Fast, simple…  ││ Version:                 │
│    click            8.4.2            Composable c…  ││   3.0.0                  │
│    itsdangerous     2.2.0            Safely pass …  ││ Requires:                │
│    Jinja2           3.1.6            A very fast …  ││   blinker, click, itsda… │
│    MarkupSafe       3.0.3            Safely add u…  ││                          │
│    Werkzeug         3.1.8            The comprehe…  ││ Direct dependencies (6): │
│                                                     ││   ✓ blinker              │
│ 1 of 54                                    all shown││   ✓ click                │
╰─────────────────────────────────────────────────────╯╰──────────────────────────╯
✓ Installed flask==3.0.0 in 1.5s — the list below has been rescanned
↑/↓ move · tab pip/apt · f filter · s search · i install · d uninstall · ? help · q quit
```

## Install

Requires a Go toolchain. The version in Debian 12 (Go 1.19) is enough — there
are no newer language features in the source.

```sh
cd tooln && go install .
```

That puts a `tooln` binary in `$(go env GOPATH)/bin`. Add it to your `PATH` if
it is not already there:

```sh
export PATH="$(go env GOPATH)/bin:$PATH"
tooln
```

## Running it

```sh
tooln              # start the interface
tooln --help       # summary of the keys
tooln --version
```

Installing and removing packages changes the machine `tooln` runs on. System
packages need root, so run it as root or under `sudo` if you intend to touch the
`apt` view. Reading the lists works fine unprivileged.

## Keys

Everything is on the keyboard; no mouse is needed. The bottom line always shows
the common keys, and `?` opens the full list with a sentence explaining each one,
so nothing has to be looked up outside the program.

| Key | What it does |
| --- | --- |
| `↑` `↓` / `k` `j` | move through the list |
| `PgUp` `PgDn`, `g` `G` | page, jump to first/last |
| `←` `→` / `h` `l` | move focus between the list and the details pane |
| `J` `K` | scroll the details pane |
| `Tab` / `Shift+Tab` | switch package manager |
| `1` / `2` | jump straight to the pip / apt view |
| `f` | filter the current list, live as you type |
| `s` or `/` | search the package index by name or keyword |
| `a` | show all results, or installed only |
| `o` | ask the manager which packages have a newer version |
| `Esc` | clear the filter, then the search, then the marks |
| `i` | install a package |
| `d` | uninstall the selected package, or every marked one |
| `U` | upgrade the selected package |
| `Space` / `x` | mark a package so one action covers several |
| `X` | clear all marks |
| `m` | maintenance menu for the current manager |
| `r` | rescan, so the list matches the real environment |
| `L` | show or hide the command log |
| `y` | print the selected package's full name and version |
| `?` | open or close the help |
| `q` / `Ctrl+C` | quit |

Nothing is changed without a confirmation that names the package and prints the
commands about to run. `Enter` or `y` goes ahead; `Esc` or `n` cancels.

## What the two views do

**pip.** Lists what `pip list` reports for the interpreter found on `PATH`, with
summaries and dependencies from `pip show`. Searching looks the name up on PyPI
and searches the [Simple index](https://peps.python.org/pep-0691/), so you can
find and install something that is not on the machine yet. Version pins work as
they do on the command line: `flask==3.0.0`, `requests>=2.31`, `requests[socks]`.

On Debian, `pip` refuses to modify the system environment (PEP 668). `tooln`
detects that at startup and, if the flag is available, adds
`--break-system-packages` so installing works as expected inside a container.
The command log shows when it does this.

**apt.** Lists what `dpkg-query` reports as fully installed — packages left in a
half-configured state are excluded, since they are not usable. Details combine
dpkg's installed record with `apt-cache show`, so both the installed version and
the version the archive offers are visible together, along with dependencies and
which installed packages depend on it. Searching uses `apt-cache search`, which
only knows about the package lists this machine has fetched; `m` then `u` runs
`apt-get update` to refresh them.

## Uninstalling and dependencies

Removing a package should not leave its dependencies behind. Both views handle
this, and both tell you what will happen before you commit:

- **pip** has no `--autoremove`, so `tooln` computes it. Before the uninstall it
  records the dependency graph from `pip show`; afterwards it works out which of
  the removed package's dependencies are no longer reachable from anything you
  still asked for, and uninstalls those too. The confirmation dialog shows this
  list up front. Reachability, rather than reference counting, is what makes a
  dependency cycle collapse instead of keeping itself alive. `pip`, `setuptools`
  and `wheel` are never removed — that would break the environment's ability to
  manage itself.
- **apt** already knows: removal uses `apt-get remove --auto-remove`.

## Layout

```
tooln/
├── main.go                    flag handling and startup
├── internal/pkgmgr/           the package managers; no UI code
│   ├── pkgmgr.go              the Manager interface and shared types
│   ├── exec.go                running commands, streaming their output
│   ├── pip.go                 pip: list, show, install, the orphan analysis
│   ├── apt.go                 apt: dpkg-query, apt-cache, apt-get
│   ├── pypi.go                the PyPI JSON and Simple index clients
│   └── parse.go               RFC 822 metadata and version parsing
└── internal/ui/               the Bubble Tea program; no package-manager code
    ├── model.go               the root model, message handling
    ├── tab.go                 per-manager state: cursor, filter, marks, cache
    ├── keys.go                every binding, with its own help text
    ├── keyhandler.go          what each key does, per input mode
    ├── view.go                layout arithmetic and the status bar
    ├── list.go                the package list
    ├── details.go             the details pane, dialogs, help and log
    └── styles.go              colours and text styles
```

The two halves do not know about each other. `pkgmgr` never formats for a
terminal, and `ui` never runs a subprocess — it asks a `Manager` for a `Plan` and
displays what comes back. Adding a third package manager means implementing one
interface.

Key bindings are declared once, in `keys.go`, together with the help text that
describes them. The status bar and the help screen are both generated from that
declaration, so a key cannot end up documented as doing something it does not do.

## Tests

```sh
go test ./...
```

The suite covers the parts that are worth pinning down:

- `internal/pkgmgr/orphan_test.go` — the dependency-collapse logic, including
  shared dependencies that must survive, chains that must fully collapse, and
  cycles, which an earlier reference-counting version got wrong.
- `internal/pkgmgr/parse_test.go` — parsing real `pip show` and `apt-cache show`
  output, including the empty-value fields that a naive continuation-line rule
  misreads; PEP 503 name normalization; PEP 508 requirement parsing; and the
  rejection of anything shell-like in a package name.
- `internal/ui/view_test.go` — renders every input mode at a range of terminal
  sizes and asserts the output is exactly the terminal height and no wider,
  that the header and tab bar are never pushed off screen, and that the key
  reminder is always present. Lip Gloss's `Height()` is a minimum rather than a
  maximum, so an over-tall panel would otherwise scroll the interface away.

The UI tests drive the model through fake managers, so they need neither a
network nor a real package manager.
