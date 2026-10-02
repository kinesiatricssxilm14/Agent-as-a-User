# toolg

`toolg` is a keyboard-driven terminal UI (TUI) for resolving Git merge
conflicts. It shows a three-way merge view — **ours / result / theirs** — lets
you pick a resolution strategy per conflict, writes the resolved file back to
the working tree, and can stage and commit the merge, all without leaving the
terminal.

It is built with [Bubble Tea](https://github.com/charmbracelet/bubbletea),
[Lip Gloss](https://github.com/charmbracelet/lipgloss), and
[Bubbles](https://github.com/charmbracelet/bubbles).

## Install

```sh
cd toolg
go install .
```

This produces the `toolg` command.

## Usage

```sh
# Resolve conflict.py in /bench/data/repo (default working directory + file)
toolg conflict.py

# Override the working directory
toolg -d /path/to/repo conflict.py

# Open the default file (conflict.py) in the default directory
toolg

# Non-interactive helpers
toolg -d /path/to/repo --list      # print conflicted files, one per line
toolg --version
```

## What it does

1. **Conflict viewing** — the open file is parsed on Git's standard markers
   (`<<<<<<<`, `=======`, `>>>>>>>`) and rendered in three side-by-side panels.
   Ours and theirs content use distinct background highlights; the currently
   selected conflict is marked with `▸` and a brighter background.
2. **Resolution strategies** — for each conflict you can choose:
   - **ours** (`o` / `1`) — keep the HEAD / current-branch side
   - **theirs** (`t` / `2`) — keep the incoming-branch side
   - **both** (`b` / `3`) — keep ours then theirs
   - **none** (`x` / `4`) — discard the conflict block
3. **Saving** (`s`) — writes the resolved content back to the file. Only the
   conflict blocks are replaced; every non-conflict line is preserved exactly.
   Unresolved blocks are left with their markers so the file stays valid.
4. **Committing** (`c`) — stages resolved files and creates the commit. During
   a merge, an empty message uses Git's prepared merge message
   (`--no-edit`); otherwise you can type a message. The commit is refused while
   any conflict markers remain.
5. **History** (`g`) — shows `git log --oneline` after the merge.

All repository operations shell out to the real `git` command, and files are
read/written with the standard library, so nothing is simulated.

## Keyboard reference

Press `?` (or `h`) inside the TUI for the full in-app reference. Summary:

| Keys | Action |
|------|--------|
| `↑` / `↓` or `k` / `j` | previous / next conflict |
| `Tab` / `Shift+Tab` | cycle panel focus |
| `[` / `]` or `PgUp` / `PgDn` | scroll focused panel |
| `Home` / `End` | top / bottom of focused panel |
| `1`/`o`, `2`/`t`, `3`/`b`, `4`/`x` | ours / theirs / both / none |
| `0`/`r` | reset conflict to unresolved |
| `O` / `T` / `B` | resolve all as ours / theirs / both |
| `s` (Ctrl+S) | save resolved content |
| `c` | stage and commit |
| `f` | conflicted-files list |
| `g` | commit history |
| `?` / `h` | help |
| `Esc` | back / cancel |
| `q` (Ctrl+C) | quit |

## Layout

```
┌ toolg · conflict.py · merge in progress ───────────────┐
│  conflict 1/2 · resolution: ours · focus: result       │
│ ┌ OURS (HEAD) ──┐ ┌ RESULT ────────┐ ┌ THEIRS (feat) ┐ │
│ │ …blue bg…     │ │ …green bg…     │ │ …red bg…      │ │
│ └───────────────┘ └────────────────┘ └───────────────┘ │
│  saved conflict.py — 1 conflict(s) still unresolved    │
│  ↑/↓ conflict · o ours · t theirs … · ? help · q quit  │
└────────────────────────────────────────────────────────┘
```

## Project structure

```
toolg/
├── main.go                 # entry point, CLI flags, TUI launch
├── internal/
│   ├── conflict/           # Git conflict-marker parser + resolution model
│   │   └── conflict_test.go
│   ├── gitx/               # thin wrappers around the real git CLI
│   └── tui/                # Bubble Tea model / view / update / styles
│       └── tui_test.go     # end-to-end tests against a real git repo
└── go.mod / go.sum
```

## Tests

```sh
go test ./...
```

The TUI tests create a real repository with a genuine merge conflict, drive the
model through its key bindings, and assert that resolution, save, commit, and
history behave correctly.
