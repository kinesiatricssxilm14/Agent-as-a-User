# tooli

`tooli` is a keyboard-only disk space visualization TUI. It scans a directory
recursively, shows a navigable list with per-entry sizes, and draws a squarified
treemap so the largest files and directories stand out visually.

## Build & install

```sh
cd tooli
cargo install --path .
```

The installed command is `tooli`.

## Run

```sh
tooli /bench/data      # scan root, overridable
tooli /some/other/dir  # any valid path
```

When no argument is given, the scan root defaults to `/bench/data`.

## Keys

| Key              | Action                                          |
|------------------|-------------------------------------------------|
| `↑` / `↓` / `j` / `k` | Move selection                             |
| `Enter`          | Open the selected directory                     |
| `←` / `Backspace` / `Esc` | Go up to the parent directory            |
| `Tab`            | Switch focus between the Entries list and the Treemap |
| `s`              | Cycle sort: size ↓ → size ↑ → name ↑            |
| `f`              | Set a minimum size filter (e.g. `100M`, `2G`)   |
| `d`              | Delete the selected file/directory (with confirm) |
| `r`              | Rescan the current tree                         |
| `h` / `?`        | Toggle extended help                            |
| `Ctrl+N`         | Grow the Top-N list                             |
| `q` / `Ctrl+C`   | Quit                                            |

## Layout

Everything is shown on one screen at once:

- **Header** — current path, total size (GB/MB, two decimals), largest item,
  file/directory counts, sort mode and active filter.
- **Entries** (left) — the current directory's children with sizes, sorted and
  filtered.
- **Treemap** (right, top) — rectangles whose area is proportional to size.
- **Top-N** (right, bottom) — the largest entries in the current directory.
- **Footer** — a shortcut legend (and prompts for filter/deletion).

All sizes come from real filesystem metadata (`std::fs`); deletion actually
removes files and directories, and the interface rescans afterwards so it always
reflects the true state of the disk.
