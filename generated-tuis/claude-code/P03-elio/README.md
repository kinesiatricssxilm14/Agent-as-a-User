# toolc

A dual-pane file manager for the terminal. The left pane lists a directory; the
right pane shows the selected entry's metadata and its **full content**. Copy,
move, rename, create directories and delete — all from the keyboard, all against
the real filesystem.

```
╭ toolc — file manager ────────────────────────────────────────────────────────────────────────────────────────────────╮
│cwd /bench/data/src  1 dir · 4 file  sort:name↑                                                                       │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ Files [5] ───────────────────────────────────╮╭ Selected ────────────────────────────────────────────────────────────╮
│nested/                                    —  ││name   config.yml                                                     │
│app.log                                 42 B  ││path   /bench/data/src/config.yml                                     │
│blob.bin                                64 B  ││type   regular file · -rw-r--r-- · 47 B (47 bytes)                    │
│config.yml                              47 B  ││mtime  2026-08-13 12:41                                               │
│notes.txt                               12 B  │╰──────────────────────────────────────────────────────────────────────╯
│                                              │╭ Preview config.yml (text) ───────────────────────────────────────────╮
│                                              ││ server:                                                              │
│                                              ││   host: 0.0.0.0                                                      │
│                                              ││   port: 8080                                                         │
│                                              ││ tls: true                                                            │
╰──────────────────────────────────────────────╯╰ 47 B · 4 lines ──────────────────────────────────────────────────────╯
 i  Press ? for help · arrows/jk to browse · Enter to open
 ↑↓/jk  move   Enter/→  open dir   ←/u  up   Tab  switch pane   c  copy   m  move   r  rename   n  new dir
 d  delete   /  filter   g  go to path   H  hidden   s  sort   w  wrap   l  activity   F5  reload   ?  help   q  quit
```

## Install

```sh
cd toolc && cargo install --path .
```

## Run

```sh
toolc                      # opens /bench/data/src
toolc /some/other/dir      # first argument overrides the working directory
toolc -a /etc              # start with dotfiles visible
toolc --help               # usage and the full key list
```

Pointing `toolc` at a *file* opens its parent directory with that file selected.

## Keys

Every binding is listed in the two-row bar at the bottom of the screen, and `?`
opens a scrollable key reference in a side column. Nothing is mouse-only.

| Navigate | |
|---|---|
| `↑` `↓` / `k` `j` | move the selection |
| `PgUp` `PgDn` | page up / down |
| `Home` `End` | first / last entry |
| `Enter` `→` `l` | open the selected directory |
| `←` `u` `Backspace` | go to the parent directory |
| `g` | go to any path |
| `~` | go to `$HOME` |
| `Tab` | switch focus between the list and the preview |

| Preview | |
|---|---|
| `↑` `↓` | scroll one line (when the preview has focus) |
| `PgUp` `PgDn` | scroll one screen |
| `Home` `End` | jump to top / bottom |
| `Shift-←` `Shift-→` | scroll horizontally (with wrap off) |
| `w` | toggle soft wrap |

| Operate | |
|---|---|
| `c` | copy to a destination path |
| `m` | move into a directory or to a new path |
| `r` | rename the selected entry |
| `n` | create a new directory (parents included) |
| `d` / `Delete` | delete, after a `y`/`n` confirmation |

| View | |
|---|---|
| `/` | filter the list by substring; `Esc` clears |
| `H` | show / hide dotfiles |
| `s` / `S` | cycle sort key / reverse the order |
| `L` | toggle the activity log |
| `F5` / `Ctrl-R` | re-read the directory from disk |
| `?` / `F1` | toggle the key reference |
| `q` / `Ctrl-C` | quit |

Inside a prompt, `Tab` completes paths against the real filesystem, `Ctrl-W`
deletes the last path segment, `Ctrl-U` clears the line, and `Esc` cancels.

## Paths

Destinations accept whatever is convenient:

- `backup.txt` — relative to the current directory
- `archive/` — an existing directory; the file keeps its name inside it
- `/bench/data/dst/out.txt` — absolute
- `~/keep/` — `~` expands to `$HOME`
- `../sibling/` — `..` is resolved

Copy and move follow `cp`/`mv` semantics: if the destination is an existing
directory the source keeps its name inside it, otherwise the destination is the
new path. Overwriting an existing path always asks first.

## Behaviour worth knowing

- **Same screen, always.** Prompts, confirmations, the help reference and the
  activity log each get their own region. Nothing floats over the file list or
  the preview, so the list, the selected file's name and its content are visible
  together at all times.
- **Real operations.** Copy uses `std::fs::copy` (byte-identical, recursive for
  directories); move uses `rename(2)` and falls back to copy-then-delete across
  filesystems; delete removes files and trees; mkdir behaves like `mkdir -p`.
  After every operation the directory is re-read from disk, so the panes never
  show a stale or simulated state.
- **Previews.** Text files are shown in full and scroll; binary files render as
  a hex dump with an ASCII gutter; directories list their children. Files are
  read up to 8 MiB, and the preview says so when it truncates.
- **Confirmations.** Deleting and overwriting require `y`. `Enter` deliberately
  does *not* confirm a delete.
- **Errors.** Permission problems, missing parents and bad paths are reported in
  the status line; the app keeps running.

## Layout

```
toolc/
├── Cargo.toml
├── src/
│   ├── main.rs      argument handling, then hands off to the library
│   ├── lib.rs       terminal setup/teardown and the event loop
│   ├── cli.rs       argument parsing, usage text, default directory
│   ├── app.rs       application state, prompts, confirmations, dispatch
│   ├── keys.rs      keyboard state machine (confirm / prompt / normal)
│   ├── listing.rs   directory reading, sorting, filtering
│   ├── preview.rs   file loading, text and hex rendering
│   ├── fs_ops.rs    the real filesystem operations
│   └── ui.rs        all rendering
├── tests/render.rs  assertions over real rendered frames
├── examples/        snapshot: dump frames as text, no terminal needed
└── scripts/         e2e_test.py: drives the built binary through a PTY
```

## Tests

```sh
cargo test                            # 80 unit + render tests
python3 scripts/e2e_test.py toolc     # 36 checks against the real binary
cargo run --example snapshot -- .      # print frames as plain text
```

`cargo test` covers the filesystem layer (byte-identical copies, move semantics,
overwrite guards, recursive delete), the listing and preview models, the key
dispatcher, and the rendered output — the render tests assert on an actual
`ratatui` frame buffer, including that a prompt or the help column never hides
the panes.

`scripts/e2e_test.py` launches the installed executable in a pseudo-terminal,
types real keystrokes, and then checks the filesystem: that a copy exists and
matches byte-for-byte, that a moved file's original path is gone, that a rename
preserves content, and that declining a confirmation changes nothing.

The build was verified end-to-end on Debian 12 slim running as root, installed
with `cargo install --path .` (see `../Dockerfile.verify`).
